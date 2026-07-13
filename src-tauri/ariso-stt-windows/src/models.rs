use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

pub(crate) const PARAKEET_MODEL_DIR: &str = "parakeet-tdt-0.6b-v3";
pub(crate) const DIARIZATION_DIR: &str = "speaker-diarization";
pub(crate) const DIARIZATION_SEGMENTATION_DIR: &str = "sherpa-onnx-pyannote-segmentation-3-0";
pub(crate) const DIARIZATION_EMBEDDING: &str =
    "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
pub(crate) const GEMMA_MODEL_DIR: &str = "gemma-3-1b-it-qat-4bit";
pub(crate) const GEMMA_GGUF: &str = "gemma-3-1b-it-q4_0.gguf";
pub(crate) const SPEECH_MODEL_VERSION: &str = "v1";
pub(crate) const NOTES_MODEL_VERSION: &str = "v2";

pub(crate) fn model_dir(models: &Path, name: &str, version: &str) -> PathBuf {
    models.join("windows").join(name).join(version)
}

#[derive(Debug)]
pub(crate) struct ParakeetPaths {
    pub(crate) encoder: PathBuf,
    pub(crate) decoder: PathBuf,
    pub(crate) joiner: PathBuf,
    pub(crate) tokens: PathBuf,
}

impl ParakeetPaths {
    pub(crate) fn discover(models: &Path) -> Result<Self> {
        let dir = model_dir(models, PARAKEET_MODEL_DIR, SPEECH_MODEL_VERSION);
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
pub(crate) struct DiarizationPaths {
    pub(crate) segmentation: PathBuf,
    pub(crate) embedding: PathBuf,
}

impl DiarizationPaths {
    pub(crate) fn discover(models: &Path) -> Option<Self> {
        let dir = model_dir(models, DIARIZATION_DIR, SPEECH_MODEL_VERSION);
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
        let dir = model_dir(temp.path(), PARAKEET_MODEL_DIR, SPEECH_MODEL_VERSION);
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
        let dir = model_dir(temp.path(), DIARIZATION_DIR, SPEECH_MODEL_VERSION);
        let segmentation = dir.join(DIARIZATION_SEGMENTATION_DIR);
        fs::create_dir_all(&segmentation).unwrap();
        fs::write(segmentation.join("model.int8.onnx"), b"segmentation").unwrap();
        fs::write(dir.join(DIARIZATION_EMBEDDING), b"embedding").unwrap();

        let paths = DiarizationPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.segmentation, segmentation.join("model.int8.onnx"));
        assert_eq!(paths.embedding, dir.join(DIARIZATION_EMBEDDING));
    }
}
