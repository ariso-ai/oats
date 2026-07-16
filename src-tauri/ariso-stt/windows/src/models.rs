//! Filesystem contract between the host model manager and Windows inference.
//!
//! Distribution paths and versions come from the shared Windows model lock.
//! The host owns acquisition and integrity checks; this module rejects an
//! incomplete installation before native inference receives any paths.

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const WINDOWS_MODEL_LOCK: &str = include_str!("../../shared/windows-models.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsModelLock {
    speech: Vec<ModelBundle>,
    notes: Vec<ModelBundle>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelBundle {
    id: String,
    install_path: String,
    files: Vec<String>,
}

fn model_lock() -> Result<WindowsModelLock> {
    serde_json::from_str(WINDOWS_MODEL_LOCK).context("parse shared Windows model lock")
}

fn bundle<'a>(bundles: &'a [ModelBundle], id: &str) -> Result<&'a ModelBundle> {
    bundles
        .iter()
        .find(|bundle| bundle.id == id)
        .ok_or_else(|| anyhow!("Windows model lock is missing bundle {id}"))
}

fn require_file(dir: &Path, relative: &str) -> Result<PathBuf> {
    let path = dir.join(relative);
    if path.is_file() {
        return Ok(path);
    }
    bail!("missing model file {}", path.display())
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
        let lock = model_lock()?;
        let definition = bundle(&lock.speech, "parakeet")?;
        let dir = models.join(&definition.install_path);
        for required in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            if !definition.files.iter().any(|file| file == required) {
                bail!("Windows model lock omits required Parakeet file {required}");
            }
        }
        Ok(Self {
            encoder: require_file(&dir, "encoder.int8.onnx")?,
            decoder: require_file(&dir, "decoder.int8.onnx")?,
            joiner: require_file(&dir, "joiner.int8.onnx")?,
            tokens: require_file(&dir, "tokens.txt")?,
        })
    }
}

#[derive(Debug)]
pub(crate) struct DiarizationPaths {
    pub(crate) segmentation: PathBuf,
    pub(crate) embedding: PathBuf,
}

impl DiarizationPaths {
    pub(crate) fn discover(models: &Path) -> Result<Self> {
        let lock = model_lock()?;
        let definition = bundle(&lock.speech, "diarization")?;
        let dir = models.join(&definition.install_path);
        Ok(Self {
            segmentation: require_file(
                &dir,
                "sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx",
            )?,
            embedding: require_file(
                &dir,
                "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx",
            )?,
        })
    }
}

pub(crate) fn discover_gemma(models: &Path) -> Result<PathBuf> {
    let lock = model_lock()?;
    let definition = bundle(&lock.notes, "gemma")?;
    let file = definition
        .files
        .first()
        .ok_or_else(|| anyhow!("Windows model lock has no Gemma file"))?;
    require_file(&models.join(&definition.install_path), file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn install_bundle(models: &Path, id: &str, files: &[&str]) -> PathBuf {
        let lock = model_lock().unwrap();
        let definition = lock
            .speech
            .iter()
            .chain(lock.notes.iter())
            .find(|bundle| bundle.id == id)
            .unwrap();
        let dir = models.join(&definition.install_path);
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        dir
    }

    #[test]
    fn shared_model_lock_is_valid() {
        let lock = model_lock().unwrap();
        assert_eq!(bundle(&lock.speech, "parakeet").unwrap().id, "parakeet");
        assert_eq!(bundle(&lock.notes, "gemma").unwrap().id, "gemma");
    }

    #[test]
    fn discovers_canonical_parakeet_layout() {
        let temp = tempfile::tempdir().unwrap();
        let dir = install_bundle(
            temp.path(),
            "parakeet",
            &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
        );
        let paths = ParakeetPaths::discover(temp.path()).unwrap();
        assert_eq!(paths.encoder, dir.join("encoder.int8.onnx"));
    }

    #[test]
    fn diarization_is_required() {
        let temp = tempfile::tempdir().unwrap();
        let dir = install_bundle(
            temp.path(),
            "diarization",
            &["sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx"],
        );
        let error = DiarizationPaths::discover(temp.path()).unwrap_err();
        assert!(error.to_string().contains("3dspeaker"));

        fs::write(
            dir.join("3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"),
            b"embedding",
        )
        .unwrap();
        assert!(DiarizationPaths::discover(temp.path()).is_ok());
    }

    #[test]
    fn discovers_qat_gemma_from_shared_lock() {
        let temp = tempfile::tempdir().unwrap();
        let dir = install_bundle(temp.path(), "gemma", &["gemma-3-1b-it-qat-Q4_0.gguf"]);
        assert_eq!(
            discover_gemma(temp.path()).unwrap(),
            dir.join("gemma-3-1b-it-qat-Q4_0.gguf")
        );
    }
}
