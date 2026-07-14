//! Filesystem contract between the host model manager and Windows inference.
//!
//! The host owns downloading, hashing, and readiness markers. This module only
//! translates the versioned installation layout into runtime paths and rejects
//! incomplete required bundles before native libraries receive them.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// Mirrors the speech bundle name published by `model_manager`; changing it is
/// a distribution migration, not merely a local runtime rename.
pub(crate) const PARAKEET_MODEL_DIR: &str = "parakeet-tdt-0.6b-v3";
/// Keeps the two diarization models under one independently versioned feature
/// bundle so speaker labels can evolve without republishing Parakeet.
pub(crate) const DIARIZATION_DIR: &str = "speaker-diarization";
/// Preserves the upstream segmentation package boundary inside the bundle;
/// deployment tooling may replace its files without changing runtime wiring.
pub(crate) const DIARIZATION_SEGMENTATION_DIR: &str = "sherpa-onnx-pyannote-segmentation-3-0";
/// Names the embedding artifact shared by clustering configuration and the R2
/// manifest. It is a deployment identity, not a user-selectable model choice.
pub(crate) const DIARIZATION_EMBEDDING: &str =
    "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
/// Aligns the notes runtime with the model-manager bundle identity while keeping
/// the Windows GGUF representation separate from macOS MLX assets.
pub(crate) const GEMMA_MODEL_DIR: &str = "gemma-3-1b-it-qat-4bit";
/// Pins the runtime-facing GGUF filename; quantization selection is made during
/// bundle publication rather than dynamically on end-user machines.
pub(crate) const GEMMA_GGUF: &str = "gemma-3-1b-it-q4_0.gguf";
/// Invalidates the complete Windows speech bundle as one compatibility unit.
/// The host must publish and mark this same revision before inference can use it.
pub(crate) const SPEECH_MODEL_VERSION: &str = "v1";
/// Encodes the versioned Windows speech namespace shared with the downloader.
/// Speech assets remain platform-specific because their native representations
/// differ from CoreML, while Gemma uses the shared logical directory below.
pub(crate) fn speech_model_dir(models: &Path, name: &str, version: &str) -> PathBuf {
    models.join("windows").join(name).join(version)
}

/// Resolves the canonical Gemma directory used by both macOS and Windows. The
/// host's `.complete` marker records which runtime-specific bundle populated it.
pub(crate) fn llm_model_dir(models: &Path) -> PathBuf {
    models.join("llm").join(GEMMA_MODEL_DIR)
}

#[derive(Debug)]
/// Carries a complete Parakeet transducer as one value so recognizer setup cannot
/// accidentally mix encoder, decoder, joiner, or token files across revisions.
pub(crate) struct ParakeetPaths {
    pub(crate) encoder: PathBuf,
    pub(crate) decoder: PathBuf,
    pub(crate) joiner: PathBuf,
    pub(crate) tokens: PathBuf,
}

/// Owns validation of the transducer as one deployable unit; recognizer setup
/// receives either a complete path set or an error, never a partial option mix.
impl ParakeetPaths {
    /// Verifies deployment completeness at the last boundary before sherpa-onnx.
    /// Cryptographic verification is intentionally absent here because the host
    /// already performs it during installation and owns repair UX.
    pub(crate) fn discover(models: &Path) -> Result<Self> {
        let dir = speech_model_dir(models, PARAKEET_MODEL_DIR, SPEECH_MODEL_VERSION);
        let paths = Self {
            encoder: dir.join("encoder.int8.onnx"),
            decoder: dir.join("decoder.int8.onnx"),
            joiner: dir.join("joiner.int8.onnx"),
            tokens: dir.join("tokens.txt"),
        };
        for path in [&paths.encoder, &paths.decoder, &paths.joiner, &paths.tokens] {
            if !path.is_file() {
                bail!("missing Parakeet model file {}", path.display());
            }
        }
        Ok(paths)
    }
}

#[derive(Debug)]
/// Groups the segmentation and embedding halves of speaker diarization. Neither
/// file is useful independently, so callers reason about feature availability
/// rather than individual artifacts.
pub(crate) struct DiarizationPaths {
    pub(crate) segmentation: PathBuf,
    pub(crate) embedding: PathBuf,
}

/// Encapsulates the product's optional-speaker-label policy at model discovery,
/// before the transcription orchestrator decides whether to use its fallback.
impl DiarizationPaths {
    /// Treats diarization as an optional enhancement to the transcript contract:
    /// missing files allow the caller to produce a single-speaker transcript.
    /// Download readiness and user-facing repair remain host responsibilities.
    pub(crate) fn discover(models: &Path) -> Option<Self> {
        let dir = speech_model_dir(models, DIARIZATION_DIR, SPEECH_MODEL_VERSION);
        let paths = Self {
            segmentation: dir
                .join(DIARIZATION_SEGMENTATION_DIR)
                .join("model.int8.onnx"),
            embedding: dir.join(DIARIZATION_EMBEDDING),
        };
        (paths.segmentation.is_file() && paths.embedding.is_file()).then_some(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_canonical_parakeet_layout() {
        let temp = tempfile::tempdir().unwrap();
        let dir = speech_model_dir(temp.path(), PARAKEET_MODEL_DIR, SPEECH_MODEL_VERSION);
        fs::create_dir_all(&dir).unwrap();
        for file in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            fs::write(dir.join(file), b"fixture").unwrap();
        }

        let paths = ParakeetPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.encoder, dir.join("encoder.int8.onnx"));
    }

    #[test]
    fn discovers_canonical_diarization_layout() {
        let temp = tempfile::tempdir().unwrap();
        let dir = speech_model_dir(temp.path(), DIARIZATION_DIR, SPEECH_MODEL_VERSION);
        let segmentation = dir.join(DIARIZATION_SEGMENTATION_DIR);
        fs::create_dir_all(&segmentation).unwrap();
        fs::write(segmentation.join("model.int8.onnx"), b"segmentation").unwrap();
        fs::write(dir.join(DIARIZATION_EMBEDDING), b"embedding").unwrap();

        let paths = DiarizationPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.segmentation, segmentation.join("model.int8.onnx"));
        assert_eq!(paths.embedding, dir.join(DIARIZATION_EMBEDDING));
    }
}
