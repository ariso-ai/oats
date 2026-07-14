//! Windows transcription and speaker-label orchestration.
//!
//! sherpa-onnx supplies the model engines, while this module translates inference
//! output into the raw `ariso-stt` contract. Cross-platform ordering, speaker-ID
//! normalization, participant labels, downloads, and persistence belong to the
//! Tauri host.

use crate::audio::{Audio, decode_audio, resample_linear, slice_audio};
use crate::models::{DiarizationPaths, ParakeetPaths};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerDiarizationSegment,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    OfflineTransducerModelConfig, SpeakerEmbeddingExtractorConfig,
};
use std::env;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
/// Mirrors the language-neutral sidecar schema shared with macOS. Keeping this
/// output engine-agnostic lets the host finalize recordings without branching
/// on Parakeet, CoreML, or future inference backends.
pub(crate) struct TranscriptOutput {
    language: String,
    duration_seconds: f64,
    segments: Vec<Segment>,
}

#[derive(Debug, Deserialize, Serialize)]
/// Preserves the model-native speaker key until the host applies the one shared
/// ordering and participant policy used by both Windows and macOS.
struct Segment {
    speaker: String,
    text: String,
    start: f64,
    end: f64,
}

/// Coordinates decoding, Parakeet, and optional diarization into one host-facing
/// result. If speaker models are unavailable or yield no usable spans, the
/// contract still degrades to a complete single-speaker transcript.
pub(crate) fn transcribe(audio_path: &Path, models: &Path) -> Result<TranscriptOutput> {
    let paths = ParakeetPaths::discover(models)?;
    let audio = decode_audio(audio_path)?;
    let diarization = DiarizationPaths::discover(models);
    if debug_diarization_enabled() {
        eprintln!(
            "diarization models: {}",
            if diarization.is_some() {
                "found"
            } else {
                "missing"
            }
        );
    }

    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(paths.encoder.to_string_lossy().to_string()),
        decoder: Some(paths.decoder.to_string_lossy().to_string()),
        joiner: Some(paths.joiner.to_string_lossy().to_string()),
    };
    config.model_config.tokens = Some(paths.tokens.to_string_lossy().to_string());
    config.model_config.model_type = Some("nemo_transducer".to_string());
    config.model_config.provider =
        Some(env::var("ARISO_STT_ONNX_PROVIDER").unwrap_or_else(|_| "cpu".to_string()));
    config.model_config.num_threads = env::var("ARISO_STT_THREADS")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or_else(default_threads);
    config.decoding_method = Some("greedy_search".to_string());

    let recognizer =
        OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("create Parakeet recognizer"))?;
    let duration = audio.duration_seconds();

    if let Some(diarization) = diarization {
        let diarized = transcribe_diarized(&recognizer, &audio, &diarization, duration)?;
        if !diarized.segments.is_empty() {
            return Ok(diarized);
        }
    }

    let text = recognize_text(&recognizer, audio.sample_rate, &audio.samples)?;

    Ok(TranscriptOutput {
        language: "en".to_string(),
        duration_seconds: duration,
        segments: vec![Segment {
            speaker: "0".to_string(),
            text,
            start: 0.0,
            end: duration,
        }],
    })
}

/// Provides the smallest reusable ASR operation for both whole recordings and
/// diarized clips. Speaker assignment and timestamps stay outside this helper so
/// Parakeet remains unaware of the transcript composition policy.
fn recognize_text(
    recognizer: &OfflineRecognizer,
    sample_rate: i32,
    samples: &[f32],
) -> Result<String> {
    if samples.is_empty() {
        bail!("cannot transcribe an empty audio segment");
    }
    let stream = recognizer.create_stream();
    stream.accept_waveform(sample_rate, samples);
    recognizer.decode(&stream);
    let result = stream
        .get_result()
        .ok_or_else(|| anyhow!("Parakeet recognizer returned no result"))?;
    let text = result.text.trim().to_string();
    if text.is_empty() {
        bail!("Parakeet recognizer returned an empty transcript");
    }
    Ok(text)
}

/// Turns anonymous speaker spans into independently recognized transcript
/// segments. It intentionally creates generic labels only; identity resolution
/// and participant enrichment would require meeting metadata outside the model.
fn transcribe_diarized(
    recognizer: &OfflineRecognizer,
    audio: &Audio,
    paths: &DiarizationPaths,
    duration: f64,
) -> Result<TranscriptOutput> {
    let provider = env::var("ARISO_DIARIZATION_ONNX_PROVIDER")
        .or_else(|_| env::var("ARISO_STT_ONNX_PROVIDER"))
        .unwrap_or_else(|_| "cpu".to_string());
    let threads = env::var("ARISO_DIARIZATION_THREADS")
        .or_else(|_| env::var("ARISO_STT_THREADS"))
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or_else(default_threads);

    let diarizer = OfflineSpeakerDiarization::create(&OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(paths.segmentation.to_string_lossy().to_string()),
            },
            num_threads: threads,
            provider: Some(provider.clone()),
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(paths.embedding.to_string_lossy().to_string()),
            num_threads: threads,
            provider: Some(provider),
            ..Default::default()
        },
        clustering: FastClusteringConfig {
            num_clusters: env::var("ARISO_DIARIZATION_SPEAKERS")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(-1),
            ..Default::default()
        },
        ..Default::default()
    })
    .ok_or_else(|| anyhow!("create speaker diarizer"))?;

    let diarization_sample_rate = diarizer.sample_rate();
    let resampled_audio;
    let diarization_audio = if diarization_sample_rate == audio.sample_rate {
        audio.samples.as_slice()
    } else {
        resampled_audio =
            resample_linear(&audio.samples, audio.sample_rate, diarization_sample_rate);
        resampled_audio.as_slice()
    };
    let diarization_result = diarizer
        .process(diarization_audio)
        .ok_or_else(|| anyhow!("speaker diarizer returned no result"))?;
    let diarization_segments =
        sanitize_diarization_segments(diarization_result.sort_by_start_time(), duration);
    if debug_diarization_enabled() {
        eprintln!(
            "diarization speakers={} segments={}",
            diarization_result.num_speakers(),
            diarization_segments.len()
        );
        for segment in &diarization_segments {
            eprintln!(
                "diarization segment {:.3}-{:.3} speaker {}",
                segment.start, segment.end, segment.speaker
            );
        }
    }

    let mut segments = Vec::new();
    let segment_padding = env_float("ARISO_DIARIZATION_SEGMENT_PADDING", 0.2);
    for diarization_segment in &diarization_segments {
        let clip = slice_audio(
            audio,
            diarization_segment.start - segment_padding,
            diarization_segment.end + segment_padding,
        );
        if clip.len() < audio.sample_rate.max(1) as usize / 4 {
            continue;
        }
        match recognize_text(recognizer, audio.sample_rate, &clip) {
            Ok(text) => segments.push(Segment {
                speaker: diarization_segment.speaker.to_string(),
                text,
                start: diarization_segment.start,
                end: diarization_segment.end,
            }),
            Err(err) => {
                if debug_diarization_enabled() {
                    eprintln!(
                        "skipping diarized segment {:.3}-{:.3}: {err:#}",
                        diarization_segment.start, diarization_segment.end
                    );
                }
            }
        }
    }

    if segments.is_empty() {
        return Ok(TranscriptOutput {
            language: "en".to_string(),
            duration_seconds: duration,
            segments,
        });
    }

    Ok(TranscriptOutput {
        language: "en".to_string(),
        duration_seconds: duration,
        segments,
    })
}

#[derive(Clone, Debug, PartialEq)]
/// Normalizes sherpa-onnx diarization output before it enters the shared schema.
/// Keeping the engine's signed speaker IDs private prevents invalid native
/// values from leaking into host storage.
struct SpeakerSpan {
    speaker: u32,
    start: f64,
    end: f64,
}

/// Clamps native-model spans to the decoded recording and rejects unusable IDs
/// or ranges. This is a trust boundary around inference output, not an attempt
/// to merge overlapping speech or improve diarization quality.
fn sanitize_diarization_segments(
    segments: Vec<OfflineSpeakerDiarizationSegment>,
    duration: f64,
) -> Vec<SpeakerSpan> {
    segments
        .into_iter()
        .filter_map(|segment| {
            let start = (segment.start as f64).clamp(0.0, duration);
            let end = (segment.end as f64).clamp(0.0, duration);
            if end <= start || segment.speaker < 0 {
                None
            } else {
                Some(SpeakerSpan {
                    speaker: segment.speaker as u32,
                    start,
                    end,
                })
            }
        })
        .collect()
}

/// Keeps verbose model diagnostics opt-in and on stderr so stdout remains valid
/// JSON for the host. This switch is for development and smoke tests, not a
/// persisted application preference.
fn debug_diarization_enabled() -> bool {
    env::var("ARISO_DEBUG_DIARIZATION").is_ok_and(|value| value != "0")
}

/// Caps default CPU parallelism to leave the desktop responsive during local
/// inference. Benchmark tooling can override it; maximum throughput is not the
/// production default on general-purpose laptops.
fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 4) as i32)
        .unwrap_or(2)
}

/// Exposes bounded numeric tuning to diagnostics without broadening the public
/// CLI contract. Invalid or negative values fall back so environment mistakes
/// cannot create nonsensical model spans.
fn env_float(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::TranscriptOutput;

    #[test]
    fn shared_fixture_matches_windows_output_contract() {
        let fixture = include_str!("../../shared/fixtures/transcript.json");
        let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let output: TranscriptOutput = serde_json::from_str(fixture).unwrap();

        assert_eq!(serde_json::to_value(output).unwrap(), expected);
    }

    #[test]
    fn shared_transcript_schema_is_valid_json() {
        let schema = include_str!("../../shared/transcript.schema.json");
        serde_json::from_str::<serde_json::Value>(schema).unwrap();
    }
}
