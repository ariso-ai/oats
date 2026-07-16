//! Windows transcription and speaker-label orchestration.
//!
//! sherpa-onnx supplies the model engines, while this module translates inference
//! output into the raw `ariso-stt` contract. Cross-platform ordering, speaker-ID
//! normalization, participant labels, downloads, and persistence belong to the
//! Tauri host.

use crate::audio::{Audio, decode_audio, resample_linear, slice_audio};
use crate::models::{DiarizationPaths, ParakeetPaths};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerDiarizationSegment,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    OfflineTransducerModelConfig, SpeakerEmbeddingExtractorConfig,
};
use std::env;
use std::path::Path;

const DIARIZATION_SEGMENT_PADDING_SECONDS: f64 = 0.2;
const FALLBACK_ASR_WINDOW_SECONDS: f64 = 30.0;
const FALLBACK_ASR_OVERLAP_SECONDS: f64 = 1.0;
const FALLBACK_DEDUP_MAX_WORDS: usize = 24;

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

/// Coordinates decoding, Parakeet, and required diarization into one host-facing
/// result. When speaker-level recognition is unreliable, the complete recording
/// is transcribed as one speaker so a diarization problem cannot omit speech.
pub(crate) fn transcribe(audio_path: &Path, models: &Path) -> Result<TranscriptOutput> {
    let paths = ParakeetPaths::discover(models)?;
    let audio = decode_audio(audio_path)?;
    let diarization = DiarizationPaths::discover(models);

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

    match diarization.and_then(|paths| transcribe_diarized(&recognizer, &audio, &paths, duration)) {
        Ok(output) => Ok(output),
        Err(error) => fall_back_to_whole_audio(&recognizer, &audio, duration, error),
    }
}

/// Provides the smallest reusable ASR operation for both whole recordings and
/// diarized clips. Speaker assignment and timestamps stay outside this helper so
/// Parakeet remains unaware of the transcript composition policy.
fn recognize_text(
    recognizer: &OfflineRecognizer,
    sample_rate: i32,
    samples: &[f32],
) -> Result<Option<String>> {
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
    Ok((!text.is_empty()).then_some(text))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AsrWindow {
    output_start: f64,
    clip_start: f64,
    end: f64,
}

/// Bounds Parakeet input duration when the speaker pipeline cannot be used.
/// Neighboring windows share a short prefix so speech crossing a boundary is
/// available in full to the later decode rather than being cut in half.
fn fallback_asr_windows(duration: f64) -> Vec<AsrWindow> {
    if !duration.is_finite() || duration <= 0.0 {
        return Vec::new();
    }

    let mut windows = Vec::new();
    let mut output_start = 0.0;
    while output_start < duration {
        let end = (output_start + FALLBACK_ASR_WINDOW_SECONDS).min(duration);
        windows.push(AsrWindow {
            output_start,
            clip_start: if output_start == 0.0 {
                0.0
            } else {
                (output_start - FALLBACK_ASR_OVERLAP_SECONDS).max(0.0)
            },
            end,
        });
        if end >= duration {
            break;
        }
        output_start = end;
    }
    windows
}

/// Removes words emitted twice because adjacent fallback windows overlap. The
/// comparison is deliberately bounded and exact so ordinary repetition later
/// in a meeting is not treated as an ASR boundary artifact.
fn remove_repeated_prefix(previous: &str, current: &str) -> String {
    let previous_words: Vec<&str> = previous.split_whitespace().collect();
    let current_words: Vec<&str> = current.split_whitespace().collect();
    let max_overlap = previous_words
        .len()
        .min(current_words.len())
        .min(FALLBACK_DEDUP_MAX_WORDS);

    for count in (1..=max_overlap).rev() {
        let previous_start = previous_words.len() - count;
        if previous_words[previous_start..]
            .iter()
            .zip(&current_words[..count])
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
        {
            return current_words[count..].join(" ");
        }
    }
    current.trim().to_string()
}

/// Preserves the recording when diarization cannot provide trustworthy speaker
/// clips. Fixed-size, overlapping windows avoid the empty result Parakeet can
/// return for long recordings while retaining one generic speaker identity.
fn transcribe_whole_audio(
    recognizer: &OfflineRecognizer,
    audio: &Audio,
    duration: f64,
) -> Result<TranscriptOutput> {
    let mut segments: Vec<Segment> = Vec::new();
    for window in fallback_asr_windows(duration) {
        let clip = slice_audio(audio, window.clip_start, window.end);
        let text = recognize_text(recognizer, audio.sample_rate, &clip).with_context(|| {
            format!(
                "transcribe fallback window {:.3}-{:.3}",
                window.clip_start, window.end
            )
        })?;
        let Some(mut text) = text else {
            continue;
        };
        if let Some(previous) = segments.last() {
            text = remove_repeated_prefix(&previous.text, &text);
        }
        if text.is_empty() {
            continue;
        }
        segments.push(Segment {
            speaker: "0".to_string(),
            text,
            start: window.output_start,
            end: window.end,
        });
    }
    if segments.is_empty() {
        bail!("Parakeet recognizer returned an empty transcript");
    }

    Ok(TranscriptOutput {
        language: "en".to_string(),
        duration_seconds: duration,
        segments,
    })
}

/// Records why speaker labels were abandoned while keeping stdout reserved for
/// the sidecar's JSON contract. Failure context includes both the regional and
/// whole-recording attempts if ASR cannot recover.
fn fall_back_to_whole_audio(
    recognizer: &OfflineRecognizer,
    audio: &Audio,
    duration: f64,
    reason: impl std::fmt::Display,
) -> Result<TranscriptOutput> {
    eprintln!(
        "speaker-level transcription unavailable ({reason}); transcribing complete audio in bounded windows"
    );
    transcribe_whole_audio(recognizer, audio, duration)
        .with_context(|| format!("whole-audio fallback after {reason}"))
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
            num_clusters: -1,
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
    if diarization_segments.is_empty() {
        bail!("speaker diarizer returned no usable segments");
    }

    let mut segments = Vec::with_capacity(diarization_segments.len());
    for diarization_segment in &diarization_segments {
        let clip = slice_audio(
            audio,
            diarization_segment.start - DIARIZATION_SEGMENT_PADDING_SECONDS,
            diarization_segment.end + DIARIZATION_SEGMENT_PADDING_SECONDS,
        );
        if clip.len() < audio.sample_rate.max(1) as usize / 4 {
            bail!(
                "diarized segment {:.3}-{:.3} is too short to transcribe",
                diarization_segment.start,
                diarization_segment.end
            );
        }
        let text = recognize_text(recognizer, audio.sample_rate, &clip)
            .with_context(|| {
                format!(
                    "transcribe diarized segment {:.3}-{:.3}",
                    diarization_segment.start, diarization_segment.end
                )
            })?
            .ok_or_else(|| {
                anyhow!(
                    "diarized segment {:.3}-{:.3} produced no speech",
                    diarization_segment.start,
                    diarization_segment.end
                )
            })?;
        segments.push(Segment {
            speaker: diarization_segment.speaker.to_string(),
            text,
            start: diarization_segment.start,
            end: diarization_segment.end,
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

/// Caps default CPU parallelism to leave the desktop responsive during local
/// inference. Benchmark tooling can override it; maximum throughput is not the
/// production default on general-purpose laptops.
fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 4) as i32)
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::{AsrWindow, TranscriptOutput, fallback_asr_windows, remove_repeated_prefix};

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

    #[test]
    fn fallback_windows_bound_long_recordings_and_overlap_boundaries() {
        assert_eq!(
            fallback_asr_windows(64.0),
            vec![
                AsrWindow {
                    output_start: 0.0,
                    clip_start: 0.0,
                    end: 30.0,
                },
                AsrWindow {
                    output_start: 30.0,
                    clip_start: 29.0,
                    end: 60.0,
                },
                AsrWindow {
                    output_start: 60.0,
                    clip_start: 59.0,
                    end: 64.0,
                },
            ]
        );
    }

    #[test]
    fn fallback_deduplicates_only_a_shared_boundary_prefix() {
        assert_eq!(
            remove_repeated_prefix("We approved the launch plan", "the launch plan Next topic"),
            "Next topic"
        );
        assert_eq!(
            remove_repeated_prefix("We approved the launch plan", "A different launch plan"),
            "A different launch plan"
        );
    }
}
