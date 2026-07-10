use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerDiarizationSegment,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    OfflineTransducerModelConfig, SpeakerEmbeddingExtractorConfig,
};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const PARAKEET_MODEL_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const PARAKEET_PRODUCT_DIR: &str = "parakeet-tdt-0.6b-v3";
const DIARIZATION_DIR: &str = "speaker-diarization";
const WINDOWS_MODEL_VERSION: &str = "v1";
const DIARIZATION_SEGMENTATION_DIR: &str = "sherpa-onnx-pyannote-segmentation-3-0";
const DIARIZATION_REVERB_DIR: &str = "sherpa-onnx-reverb-diarization-v1";
const DIARIZATION_3D_SPEAKER: &str = "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
const DIARIZATION_NEMO_SPEAKER: &str = "nemo_en_titanet_small.onnx";
const PARAKEET_DOWNLOAD_BASE: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main";
const SHERPA_SEGMENTATION_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const SHERPA_3D_SPEAKER_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
const GEMMA_MODEL_DIR: &str = "gemma-3-1b-it-qat-4bit";
const GEMMA_QAT_GGUF: &str = "gemma-3-1b-it-q4_0.gguf";
const GEMMA_LEGACY_GGUF: &str = "gemma-3-1b-it-qat-q4_0.gguf";
const GEMMA_SPIKE_GGUF_URL: &str =
    "https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/resolve/main/gemma-3-1b-it-Q4_K_M.gguf";
const LLAMA_WIN_CPU_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9940/llama-b9940-bin-win-cpu-x64.zip";
const ENV_PARAKEET_BASE_URL: &str = "ARISO_WINDOWS_PARAKEET_BASE_URL";
const ENV_DIARIZATION_SEGMENTATION_URL: &str = "ARISO_WINDOWS_DIARIZATION_SEGMENTATION_URL";
const ENV_DIARIZATION_EMBEDDING_URL: &str = "ARISO_WINDOWS_DIARIZATION_EMBEDDING_URL";
const ENV_GEMMA_GGUF_URL: &str = "ARISO_WINDOWS_GEMMA_GGUF_URL";
const ENV_LLAMA_RUNTIME_URL: &str = "ARISO_WINDOWS_LLAMA_RUNTIME_URL";

fn usage() -> &'static str {
    "ariso-stt Windows Parakeet local spike\n\n\
     Contract:\n\
       ariso-stt --audio <path> --models <dir> --format json\n\
       ariso-stt download --models <dir>\n\
       ariso-stt download-notes --models <dir>\n\
       ariso-stt notes --transcript <path> --models <dir>\n"
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", usage());
        return Ok(());
    }

    match parse_args(&args)? {
        SidecarCommand::Download { models } => download_models(&models),
        SidecarCommand::DownloadNotes { models } => download_notes_models(&models),
        SidecarCommand::Notes { transcript, models } => run_notes(&transcript, &models),
        SidecarCommand::Transcribe {
            audio,
            models,
            format,
        } => {
            if format != "json" {
                bail!("unsupported format {format:?}; expected json");
            }
            let output = transcribe(&audio, &models)?;
            println!("{}", serde_json::to_string(&output)?);
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq)]
enum SidecarCommand {
    Download {
        models: PathBuf,
    },
    DownloadNotes {
        models: PathBuf,
    },
    Notes {
        transcript: PathBuf,
        models: PathBuf,
    },
    Transcribe {
        audio: PathBuf,
        models: PathBuf,
        format: String,
    },
}

fn parse_args(args: &[String]) -> Result<SidecarCommand> {
    if args.first().is_some_and(|arg| arg == "download") {
        return Ok(SidecarCommand::Download {
            models: required_path_arg(&args[1..], "--models")?,
        });
    }

    if args.first().is_some_and(|arg| arg == "download-notes") {
        return Ok(SidecarCommand::DownloadNotes {
            models: required_path_arg(&args[1..], "--models")?,
        });
    }

    if args.first().is_some_and(|arg| arg == "notes") {
        return Ok(SidecarCommand::Notes {
            transcript: required_path_arg(&args[1..], "--transcript")?,
            models: required_path_arg(&args[1..], "--models")?,
        });
    }

    if args.iter().any(|arg| arg == "--audio") {
        return Ok(SidecarCommand::Transcribe {
            audio: required_path_arg(args, "--audio")?,
            models: required_path_arg(args, "--models")?,
            format: required_string_arg(args, "--format")?,
        });
    }

    bail!("{}", usage())
}

fn required_path_arg(args: &[String], flag: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(required_string_arg(args, flag)?))
}

fn required_string_arg(args: &[String], flag: &str) -> Result<String> {
    let pos = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| anyhow!("missing required {flag} argument"))?;
    args.get(pos + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| anyhow!("missing value for {flag}"))
}

#[derive(Debug)]
struct ParakeetPaths {
    encoder: PathBuf,
    decoder: PathBuf,
    joiner: PathBuf,
    tokens: PathBuf,
}

#[derive(Debug)]
struct DiarizationPaths {
    segmentation: PathBuf,
    embedding: PathBuf,
}

impl DiarizationPaths {
    fn discover(models: &Path) -> Option<Self> {
        let roots = [
            windows_versioned_dir(models, DIARIZATION_DIR),
            windows_unversioned_dir(models, DIARIZATION_DIR),
            models.join(DIARIZATION_DIR),
            models.to_path_buf(),
        ];

        for root in roots {
            let segmentation = [
                root.join(DIARIZATION_SEGMENTATION_DIR)
                    .join("model.int8.onnx"),
                root.join(DIARIZATION_SEGMENTATION_DIR).join("model.onnx"),
                root.join(DIARIZATION_REVERB_DIR).join("model.int8.onnx"),
                root.join(DIARIZATION_REVERB_DIR).join("model.onnx"),
                root.join("segmentation").join("model.int8.onnx"),
                root.join("segmentation").join("model.onnx"),
            ]
            .into_iter()
            .find(|path| path.is_file());

            let embedding = [
                root.join(DIARIZATION_3D_SPEAKER),
                root.join(DIARIZATION_NEMO_SPEAKER),
                root.join("embedding").join(DIARIZATION_3D_SPEAKER),
                root.join("embedding").join(DIARIZATION_NEMO_SPEAKER),
            ]
            .into_iter()
            .find(|path| path.is_file());

            if let (Some(segmentation), Some(embedding)) = (segmentation, embedding) {
                return Some(Self {
                    segmentation,
                    embedding,
                });
            }
        }

        None
    }
}

impl ParakeetPaths {
    fn discover(models: &Path) -> Result<Self> {
        let candidates = [
            windows_versioned_dir(models, PARAKEET_PRODUCT_DIR),
            windows_unversioned_dir(models, PARAKEET_PRODUCT_DIR),
            models.join(PARAKEET_MODEL_DIR),
            models.join(PARAKEET_PRODUCT_DIR),
        ];
        let dir = candidates
            .into_iter()
            .find(|candidate| candidate.join("encoder.int8.onnx").is_file())
            .ok_or_else(|| {
                anyhow!(
                    "Parakeet model not found under {}; expected windows/{PARAKEET_PRODUCT_DIR}/{WINDOWS_MODEL_VERSION} or {PARAKEET_MODEL_DIR}",
                    models.display()
                )
            })?;
        let paths = Self {
            encoder: dir.join("encoder.int8.onnx"),
            decoder: dir.join("decoder.int8.onnx"),
            joiner: dir.join("joiner.int8.onnx"),
            tokens: dir.join("tokens.txt"),
        };
        paths.validate()?;
        Ok(paths)
    }

    fn validate(&self) -> Result<()> {
        for path in [&self.encoder, &self.decoder, &self.joiner, &self.tokens] {
            if !path.is_file() {
                bail!("missing Parakeet model file {}", path.display());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Audio {
    sample_rate: i32,
    samples: Vec<f32>,
}

impl Audio {
    fn duration_seconds(&self) -> f64 {
        if self.sample_rate <= 0 {
            0.0
        } else {
            self.samples.len() as f64 / self.sample_rate as f64
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptOutput {
    language: String,
    duration_seconds: f64,
    participants: Vec<Participant>,
    segments: Vec<Segment>,
}

#[derive(Debug, Serialize)]
struct Participant {
    id: u32,
    label: String,
}

#[derive(Debug, Serialize)]
struct Segment {
    speaker: u32,
    text: String,
    start: f64,
    end: f64,
}

fn transcribe(audio_path: &Path, models: &Path) -> Result<TranscriptOutput> {
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
        participants: vec![Participant {
            id: 0,
            label: "Speaker 1".to_string(),
        }],
        segments: vec![Segment {
            speaker: 0,
            text,
            start: 0.0,
            end: duration,
        }],
    })
}

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
    let diarization_audio =
        resample_linear(&audio.samples, audio.sample_rate, diarization_sample_rate);
    let diarization_result = diarizer
        .process(&diarization_audio)
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
                speaker: diarization_segment.speaker,
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
            participants: Vec::new(),
            segments,
        });
    }

    segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut speaker_ids = segments
        .iter()
        .map(|segment| segment.speaker)
        .collect::<Vec<_>>();
    speaker_ids.sort_unstable();
    speaker_ids.dedup();
    for segment in &mut segments {
        segment.speaker = speaker_ids
            .iter()
            .position(|speaker| *speaker == segment.speaker)
            .unwrap_or_default() as u32;
    }
    let participants = (0..speaker_ids.len() as u32)
        .map(|id| Participant {
            id,
            label: format!("Speaker {}", id + 1),
        })
        .collect();

    Ok(TranscriptOutput {
        language: "en".to_string(),
        duration_seconds: duration,
        participants,
        segments,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct SpeakerSpan {
    speaker: u32,
    start: f64,
    end: f64,
}

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

fn slice_audio(audio: &Audio, start: f64, end: f64) -> Vec<f32> {
    let sample_rate = audio.sample_rate.max(1) as f64;
    let start_index = ((start.max(0.0) * sample_rate).floor() as usize).min(audio.samples.len());
    let end_index = ((end.max(start) * sample_rate).ceil() as usize).min(audio.samples.len());
    audio.samples[start_index..end_index].to_vec()
}

fn resample_linear(samples: &[f32], from_rate: i32, to_rate: i32) -> Vec<f32> {
    if samples.is_empty() || from_rate <= 0 || to_rate <= 0 || from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((samples.len() as f64 * ratio).round() as usize).max(1);
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let source = index as f64 / ratio;
        let left = source.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (source - left as f64) as f32;
        out.push(samples[left] * (1.0 - fraction) + samples[right] * fraction);
    }
    out
}

fn debug_diarization_enabled() -> bool {
    env::var("ARISO_DEBUG_DIARIZATION").is_ok_and(|value| value != "0")
}

fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 4) as i32)
        .unwrap_or(2)
}

fn decode_audio(path: &Path) -> Result<Audio> {
    let file = fs::File::open(path).with_context(|| format!("open audio {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("probe audio {}", path.display()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no supported audio track in {}", path.display()))?
        .clone();
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("audio sample rate missing in {}", path.display()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .with_context(|| format!("create decoder for {}", path.display()))?;
    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => bail!("audio decoder reset required"),
            Err(err) => return Err(err).with_context(|| format!("read packet {}", path.display())),
        };
        if packet.track_id() != track.id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(err).with_context(|| format!("decode {}", path.display())),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let buf =
            sample_buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(channels) {
            let sum: f32 = frame.iter().copied().sum();
            samples.push(sum / frame.len() as f32);
        }
    }

    if samples.is_empty() {
        bail!("decoded no samples from {}", path.display());
    }

    Ok(Audio {
        sample_rate: sample_rate as i32,
        samples,
    })
}

fn download_models(models: &Path) -> Result<()> {
    fs::create_dir_all(models)
        .with_context(|| format!("create models dir {}", models.display()))?;
    download_windows_speech_bundle(models)?;
    ParakeetPaths::discover(models)?;
    DiarizationPaths::discover(models).ok_or_else(|| {
        anyhow!(
            "speaker diarization artifacts not found under {}; expected windows/{DIARIZATION_DIR}/{WINDOWS_MODEL_VERSION}/{DIARIZATION_SEGMENTATION_DIR}/model.int8.onnx and {DIARIZATION_3D_SPEAKER}",
            models.display()
        )
    })?;
    eprintln!(
        "Windows Parakeet and speaker diarization artifacts are present under {}",
        models.display()
    );
    Ok(())
}

struct DownloadFile<'a> {
    url: String,
    path: &'a Path,
    size: u64,
    sha256: &'static str,
}

fn download_windows_speech_bundle(models: &Path) -> Result<()> {
    let parakeet = windows_versioned_dir(models, PARAKEET_PRODUCT_DIR);
    fs::create_dir_all(&parakeet)
        .with_context(|| format!("create Parakeet dir {}", parakeet.display()))?;
    for (file, size, sha256) in [
        (
            "encoder.int8.onnx",
            652_184_281,
            "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247",
        ),
        (
            "decoder.int8.onnx",
            11_845_275,
            "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e",
        ),
        (
            "joiner.int8.onnx",
            6_355_277,
            "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3",
        ),
        (
            "tokens.txt",
            93_939,
            "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        ),
    ] {
        let path = parakeet.join(file);
        download_verified(DownloadFile {
            url: download_url_from_base(ENV_PARAKEET_BASE_URL, PARAKEET_DOWNLOAD_BASE, file),
            path: &path,
            size,
            sha256,
        })?;
    }

    let diarization = windows_versioned_dir(models, DIARIZATION_DIR);
    fs::create_dir_all(&diarization)
        .with_context(|| format!("create diarization dir {}", diarization.display()))?;
    let archive = diarization.join(format!("{DIARIZATION_SEGMENTATION_DIR}.tar.bz2"));
    let segmentation_model = diarization
        .join(DIARIZATION_SEGMENTATION_DIR)
        .join("model.int8.onnx");
    if !segmentation_model.is_file() {
        download_verified(DownloadFile {
            url: download_url_override(
                ENV_DIARIZATION_SEGMENTATION_URL,
                SHERPA_SEGMENTATION_ARCHIVE_URL,
            ),
            path: &archive,
            size: 6_958_444,
            sha256: "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488",
        })?;
        let file =
            fs::File::open(&archive).with_context(|| format!("open {}", archive.display()))?;
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut tar = tar::Archive::new(decoder);
        tar.unpack(&diarization)
            .with_context(|| format!("extract {}", archive.display()))?;
    }

    let embedding = diarization.join(DIARIZATION_3D_SPEAKER);
    download_verified(DownloadFile {
        url: download_url_override(ENV_DIARIZATION_EMBEDDING_URL, SHERPA_3D_SPEAKER_URL),
        path: &embedding,
        size: 39_593_761,
        sha256: "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b",
    })?;

    Ok(())
}

fn download_verified(file: DownloadFile<'_>) -> Result<()> {
    if file.path.is_file() && verify_file(file.path, file.size, file.sha256).is_ok() {
        return Ok(());
    }

    if let Some(parent) = file.path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let part = file.path.with_extension(format!(
        "{}.part",
        file.path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("download")
    ));
    eprintln!("downloading {}", file.url);
    let response = ureq::get(&file.url)
        .call()
        .map_err(|err| anyhow!("download {}: {err}", file.url))?;
    let mut reader = response.into_reader();
    let mut out = fs::File::create(&part).with_context(|| format!("create {}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("read {}", file.url))?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > file.size {
            let _ = fs::remove_file(&part);
            bail!(
                "{} exceeded expected size {} bytes",
                file.path.display(),
                file.size
            );
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])
            .with_context(|| format!("write {}", part.display()))?;
    }
    out.flush()
        .with_context(|| format!("flush {}", part.display()))?;
    let actual = format!("{:x}", hasher.finalize());
    if written != file.size || actual != file.sha256 {
        let _ = fs::remove_file(&part);
        bail!(
            "download verification failed for {}; expected {} bytes sha256 {}, got {} bytes sha256 {}",
            file.path.display(),
            file.size,
            file.sha256,
            written,
            actual
        );
    }
    fs::rename(&part, file.path)
        .with_context(|| format!("move {} to {}", part.display(), file.path.display()))?;
    Ok(())
}

fn verify_file(path: &Path, expected_size: u64, expected_sha256: &str) -> Result<()> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if total != expected_size || actual != expected_sha256 {
        bail!(
            "verification failed for {}; expected {} bytes sha256 {}, got {} bytes sha256 {}",
            path.display(),
            expected_size,
            expected_sha256,
            total,
            actual
        );
    }
    Ok(())
}

fn download_url_override(env_key: &str, default: &str) -> String {
    env::var(env_key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn download_url_from_base(env_key: &str, default_base: &str, file: &str) -> String {
    format!(
        "{}/{}",
        download_url_override(env_key, default_base).trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}

fn download_notes_models(models: &Path) -> Result<()> {
    let dir = windows_versioned_dir(models, GEMMA_MODEL_DIR);
    fs::create_dir_all(&dir).with_context(|| format!("create Gemma dir {}", dir.display()))?;

    let gemma = dir.join(GEMMA_QAT_GGUF);
    download_verified(DownloadFile {
        url: download_url_override(ENV_GEMMA_GGUF_URL, GEMMA_SPIKE_GGUF_URL),
        path: &gemma,
        size: 806_058_240,
        sha256: "8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135",
    })?;

    let llama_cli = dir.join(if cfg!(target_os = "windows") {
        "llama-cli.exe"
    } else {
        "llama-cli"
    });
    if !llama_cli.is_file() {
        let archive = dir.join("llama-b9940-bin-win-cpu-x64.zip");
        download_verified(DownloadFile {
            url: download_url_override(ENV_LLAMA_RUNTIME_URL, LLAMA_WIN_CPU_URL),
            path: &archive,
            size: 18_216_976,
            sha256: "d5d7248c7aacaeb0c8f15311acb0f1081874aa7a5de55843702e9e2394a05788",
        })?;
        extract_zip(&archive, &dir)?;
    }

    discover_gemma(models)?;
    discover_notes_runtime(models)?;
    eprintln!(
        "Windows Gemma GGUF and llama.cpp CPU runtime are present under {}",
        dir.display()
    );
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let mut zip =
        zip::ZipArchive::new(file).with_context(|| format!("read zip {}", archive.display()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .with_context(|| format!("read zip entry {index} from {}", archive.display()))?;
        let Some(enclosed) = entry.enclosed_name() else {
            bail!("zip entry escapes destination: {}", entry.name());
        };
        let out_path = dest.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .with_context(|| format!("create {}", out_path.display()))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let mut out = fs::File::create(&out_path)
            .with_context(|| format!("create {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("extract {}", out_path.display()))?;
    }
    Ok(())
}

fn run_notes(transcript: &Path, models: &Path) -> Result<()> {
    let gemma = discover_gemma(models)?;
    let runtime = discover_notes_runtime(models)?;
    let transcript = fs::read_to_string(transcript)
        .with_context(|| format!("read transcript {}", transcript.display()))?;
    let notes = match runtime {
        NotesRuntime::LlamaCli(llama_cli) => run_llama_notes(&llama_cli, &gemma, &transcript)?,
        NotesRuntime::ArisoRunner(runner) => run_ariso_notes_runner(&runner, &gemma, &transcript)?,
    };
    print!("{}", clean_notes(&notes));
    Ok(())
}

fn run_ariso_notes_runner(runner: &Path, gemma: &Path, transcript: &str) -> Result<String> {
    let transcript_file = tempfile_transcript(transcript)?;
    let output = Command::new(runner)
        .arg("--model")
        .arg(gemma)
        .arg("--transcript")
        .arg(&transcript_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("spawn Gemma notes runner {}", runner.display()))?;
    let _ = fs::remove_file(&transcript_file);
    if !output.status.success() {
        bail!(
            "Gemma notes runner failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn tempfile_transcript(transcript: &str) -> Result<PathBuf> {
    let dir = env::temp_dir();
    let path = dir.join(format!(
        "ariso-notes-{}-{}.md",
        std::process::id(),
        monotonic_millis()
    ));
    fs::write(&path, transcript).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn monotonic_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn run_llama_notes(llama_cli: &Path, gemma: &Path, transcript: &str) -> Result<String> {
    let prompt = gemma_notes_prompt(transcript);
    let max_tokens = env_positive("ARISO_NOTES_MAX_TOKENS", 512);
    let ctx_size = env_positive("ARISO_NOTES_CTX_SIZE", 4096);
    let output = Command::new(llama_cli)
        .arg("-m")
        .arg(gemma)
        .arg("-p")
        .arg(prompt)
        .arg("-n")
        .arg(max_tokens.to_string())
        .arg("-c")
        .arg(ctx_size.to_string())
        .arg("--temp")
        .arg("0.3")
        .arg("--repeat-penalty")
        .arg("1.15")
        .arg("--no-display-prompt")
        .arg("-no-cnv")
        .arg("--single-turn")
        .arg("--simple-io")
        .arg("--log-disable")
        .arg("--no-warmup")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("spawn llama.cpp notes runtime {}", llama_cli.display()))?;
    if !output.status.success() {
        bail!(
            "llama.cpp Gemma notes runtime failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let notes = clean_llama_output(&String::from_utf8_lossy(&output.stdout));
    if notes.is_empty() {
        bail!("llama.cpp Gemma notes runtime returned empty notes");
    }
    Ok(notes)
}

fn env_positive(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_float(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(default)
}

fn gemma_notes_prompt(transcript: &str) -> String {
    let instructions = "\
You are a meeting-notes assistant. You are given a meeting transcript and you write concise meeting notes in Markdown.\n\n\
Rules:\n\
- Use only facts stated in the transcript. Never invent details, names, or speakers.\n\
- The transcript labels speakers generically (e.g. \"Speaker 1\", \"Speaker 2\"). Do not invent any speaker or person who does not appear in the transcript.\n\
- Output the notes only, with no preamble, no closing remarks, and never repeat or restate these instructions.\n\
- Output raw Markdown directly. Never wrap the notes in a code fence.\n\
- Use these level-2 (##) sections, in this order: Summary, Key Points, Decisions, Action Items.\n\
- \"Summary\" is 2-3 sentences describing what the meeting was about. The other sections are bullet lists.\n\
- For each action item, state the task. Only attribute it to a speaker if that exact speaker explicitly committed to it in the transcript; otherwise give the task with no owner.\n\
- Omit any section that has no real content in the transcript. Never write placeholder text under a heading.";

    format!(
        "<bos><start_of_turn>user\n{instructions}\n\nTranscript:\n{transcript}<end_of_turn>\n<start_of_turn>model\n"
    )
}

fn strip_code_fences(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn clean_llama_output(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n");
    let mut seen_prompt = false;
    let mut kept = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("> ") {
            seen_prompt = true;
            kept.clear();
            continue;
        }
        if !seen_prompt {
            continue;
        }
        if trimmed.starts_with("[ Prompt:") || trimmed == "Exiting..." {
            break;
        }
        if !trimmed.is_empty() || !kept.is_empty() {
            kept.push(line);
        }
    }

    let cleaned = clean_notes(&kept.join("\n"));
    if cleaned.is_empty() {
        clean_notes(raw)
    } else {
        cleaned
    }
}

fn clean_notes(raw: &str) -> String {
    let without_fences = strip_code_fences(raw);
    for heading in [
        "## Summary",
        "## Key Points",
        "## Decisions",
        "## Action Items",
    ] {
        if let Some(index) = without_fences.find(heading) {
            return without_fences[index..].trim().to_string();
        }
    }
    without_fences
}

fn discover_gemma(models: &Path) -> Result<PathBuf> {
    let candidates = windows_gemma_dirs(models)
        .into_iter()
        .flat_map(|dir| [dir.join(GEMMA_QAT_GGUF), dir.join(GEMMA_LEGACY_GGUF)]);
    candidates.into_iter().find(|path| path.is_file()).ok_or_else(|| {
        anyhow!(
            "Gemma GGUF model not found under {}; expected windows/{GEMMA_MODEL_DIR}/{GEMMA_QAT_GGUF}",
            models.display()
        )
    })
}

#[derive(Debug, PartialEq)]
enum NotesRuntime {
    LlamaCli(PathBuf),
    ArisoRunner(PathBuf),
}

fn discover_notes_runtime(models: &Path) -> Result<NotesRuntime> {
    if let Some(path) = env::var_os("ARISO_GEMMA_NOTES_RUNNER") {
        let runner = PathBuf::from(path);
        if runner.is_file() {
            return Ok(NotesRuntime::ArisoRunner(runner));
        }
        bail!(
            "ARISO_GEMMA_NOTES_RUNNER does not point to a file: {}",
            runner.display()
        );
    }

    let exe_name = if cfg!(target_os = "windows") {
        "ariso-gemma-notes.exe"
    } else {
        "ariso-gemma-notes"
    };
    let llama_name = if cfg!(target_os = "windows") {
        "llama-cli.exe"
    } else {
        "llama-cli"
    };

    for dir in windows_gemma_dirs(models) {
        let llama = dir.join(llama_name);
        if llama.is_file() {
            return Ok(NotesRuntime::LlamaCli(llama));
        }
        let runner = dir.join(exe_name);
        if runner.is_file() {
            return Ok(NotesRuntime::ArisoRunner(runner));
        }
    }

    bail!(
        "Gemma notes runtime not found under {}; expected llama-cli next to the Windows Gemma model",
        models.display()
    )
}

fn windows_gemma_dirs(models: &Path) -> [PathBuf; 3] {
    [
        windows_versioned_dir(models, GEMMA_MODEL_DIR),
        windows_unversioned_dir(models, GEMMA_MODEL_DIR),
        models.join(GEMMA_MODEL_DIR),
    ]
}

fn windows_versioned_dir(models: &Path, name: &str) -> PathBuf {
    windows_unversioned_dir(models, name).join(WINDOWS_MODEL_VERSION)
}

fn windows_unversioned_dir(models: &Path, name: &str) -> PathBuf {
    models.join("windows").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transcribe_contract() {
        let args = vec![
            "--audio".to_string(),
            "meeting.mp3".to_string(),
            "--models".to_string(),
            "models".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::Transcribe {
                audio: PathBuf::from("meeting.mp3"),
                models: PathBuf::from("models"),
                format: "json".to_string(),
            }
        );
    }

    #[test]
    fn parses_notes_contract() {
        let args = vec![
            "notes".to_string(),
            "--transcript".to_string(),
            "transcript.md".to_string(),
            "--models".to_string(),
            "models".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::Notes {
                transcript: PathBuf::from("transcript.md"),
                models: PathBuf::from("models"),
            }
        );
    }

    #[test]
    fn parses_download_notes_contract() {
        let args = vec![
            "download-notes".to_string(),
            "--models".to_string(),
            "models".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::DownloadNotes {
                models: PathBuf::from("models"),
            }
        );
    }

    #[test]
    fn discovers_parakeet_model_layouts() {
        let temp = tempfile::tempdir().unwrap();
        let model = windows_versioned_dir(temp.path(), PARAKEET_PRODUCT_DIR);
        fs::create_dir_all(&model).unwrap();
        for file in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            fs::write(model.join(file), b"fixture").unwrap();
        }
        let paths = ParakeetPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.encoder, model.join("encoder.int8.onnx"));
    }

    #[test]
    fn discovers_diarization_model_layout() {
        let temp = tempfile::tempdir().unwrap();
        let root = windows_versioned_dir(temp.path(), DIARIZATION_DIR);
        let segmentation = root.join(DIARIZATION_SEGMENTATION_DIR);
        fs::create_dir_all(&segmentation).unwrap();
        fs::write(segmentation.join("model.int8.onnx"), b"segmentation").unwrap();
        fs::write(root.join(DIARIZATION_3D_SPEAKER), b"embedding").unwrap();

        let paths = DiarizationPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.segmentation, segmentation.join("model.int8.onnx"));
        assert_eq!(paths.embedding, root.join(DIARIZATION_3D_SPEAKER));
    }

    #[test]
    fn resamples_and_slices_audio_for_diarized_segments() {
        let audio = Audio {
            sample_rate: 4,
            samples: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        };
        assert_eq!(slice_audio(&audio, 0.25, 1.25), vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            resample_linear(&[0.0, 2.0, 4.0], 3, 6),
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 4.0]
        );
    }

    #[test]
    fn discovers_gemma_qat_gguf_and_llama_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let model = windows_versioned_dir(temp.path(), GEMMA_MODEL_DIR);
        fs::create_dir_all(&model).unwrap();
        fs::write(model.join(GEMMA_QAT_GGUF), b"gguf").unwrap();
        fs::write(
            model.join(if cfg!(target_os = "windows") {
                "llama-cli.exe"
            } else {
                "llama-cli"
            }),
            b"runtime",
        )
        .unwrap();

        assert_eq!(
            discover_gemma(temp.path()).unwrap(),
            model.join(GEMMA_QAT_GGUF)
        );
        assert_eq!(
            discover_notes_runtime(temp.path()).unwrap(),
            NotesRuntime::LlamaCli(model.join(if cfg!(target_os = "windows") {
                "llama-cli.exe"
            } else {
                "llama-cli"
            }))
        );
    }

    #[test]
    fn extracts_llama_runtime_zip() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("runtime.zip");
        {
            let file = fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("llama-cli.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"exe").unwrap();
            zip.start_file(
                "nested/runtime.dll",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(b"dll").unwrap();
            zip.finish().unwrap();
        }

        let dest = temp.path().join("out");
        extract_zip(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("llama-cli.exe")).unwrap(), b"exe");
        assert_eq!(
            fs::read(dest.join("nested").join("runtime.dll")).unwrap(),
            b"dll"
        );
    }

    #[test]
    fn download_base_url_normalizes_slashes() {
        assert_eq!(
            download_url_from_base(
                "ARISO_TEST_UNUSED",
                "https://cdn.example/models/",
                "/tokens.txt"
            ),
            "https://cdn.example/models/tokens.txt"
        );
    }

    #[test]
    fn notes_prompt_uses_gemma_turn_markers() {
        let prompt = gemma_notes_prompt("Speaker 1: Ship it.");
        assert!(prompt.starts_with("<bos><start_of_turn>user\n"));
        assert!(prompt.contains("Transcript:\nSpeaker 1: Ship it."));
        assert!(prompt.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn strips_markdown_code_fences() {
        let cleaned = strip_code_fences("```markdown\n## Summary\n- Done\n```\n");
        assert_eq!(cleaned, "## Summary\n- Done");
    }

    #[test]
    fn cleans_llama_cli_banner_and_perf_footer() {
        let raw = "Loading model...\n\n> <prompt>\n## Summary\n- Done\n\n[ Prompt: 20.0 t/s | Generation: 5.0 t/s ]\n\nExiting...\n";
        assert_eq!(clean_llama_output(raw), "## Summary\n- Done");
    }

    #[test]
    fn cleans_prompt_echo_before_notes_heading() {
        let raw = "Rules:\n- Use sections.\n## Summary\n- Done";
        assert_eq!(clean_notes(raw), "## Summary\n- Done");
    }
}
