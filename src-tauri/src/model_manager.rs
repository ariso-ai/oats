use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::storage::MODEL_VERSION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the on-device notes LLM (gemma) has been downloaded. Reported
    /// separately from `state` so the Settings window can show the LLM's own
    /// download status alongside the overall model status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_ready: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub version: String,
    pub downloaded_at: String,
}

fn manifest_path(root: &Path) -> std::path::PathBuf {
    crate::storage::models_dir(root).join("manifest.json")
}

/// Ready = a manifest ready-marker exists and parses.
///
/// This is presence-based by design: FluidAudio owns the on-disk model layout
/// (it lays out its own repo-named subdirs under the models dir and resolves
/// them internally), so Rust does not enumerate individual model files. The
/// marker is written only after the sidecar download exits successfully. A
/// user who manually deletes model files while leaving the marker would hit a
/// late failure at transcribe time (recording is retained as `failed`) rather
/// than being gated up front — an accepted v1 simplification.
pub fn is_ready(root: &Path) -> bool {
    read_manifest(root).is_some()
}

pub fn read_manifest(root: &Path) -> Option<ModelManifest> {
    let bytes = std::fs::read(manifest_path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_manifest(root: &Path, downloaded_at: &str) -> Result<(), String> {
    let dir = crate::storage::models_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create models dir: {e}"))?;
    let manifest = ModelManifest {
        version: MODEL_VERSION.to_string(),
        downloaded_at: downloaded_at.to_string(),
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(&manifest_path(root), json.as_bytes())
}

/// The notes LLM is downloaded (from the app CDN) into this directory and
/// loaded by the sidecar `notes` command via a directory-based config.
const LLM_MODEL_NAME: &str = "gemma-3-1b-it-qat-4bit";

fn llm_dir(root: &Path) -> std::path::PathBuf {
    crate::storage::models_dir(root)
        .join("llm")
        .join(LLM_MODEL_NAME)
}

/// Readiness for the notes LLM. Marker-based (not mere presence): the marker is
/// written only after every file finishes downloading, so an interrupted
/// download is not mistaken for a ready model.
pub fn llm_is_ready(root: &Path) -> bool {
    llm_dir(root).join(".complete").exists()
}

pub fn status(root: &Path) -> ModelStatus {
    let llm_ready = Some(llm_is_ready(root));
    match read_manifest(root) {
        Some(m) => ModelStatus { state: "ready".into(), version: Some(m.version), llm_ready },
        None => ModelStatus { state: "not_downloaded".into(), version: None, llm_ready },
    }
}

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Prevents concurrent model downloads (which would race on manifest.tmp).
static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn local_model_status() -> Result<ModelStatus, String> {
    let root = crate::storage::ariso_root()?;
    Ok(status(&root))
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressLine {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    fraction: Option<f64>,
}

/// Run the sidecar `download --target <target>`, streaming progress as the
/// given events. `write_manifest_after` controls whether the STT readiness
/// manifest is written on success (true for STT; the LLM uses disk-presence
/// readiness instead). A single global guard serializes downloads.
async fn run_download(
    app: tauri::AppHandle,
    target: &str,
    write_manifest_after: bool,
    ev_progress: &str,
    ev_done: &str,
    ev_error: &str,
) -> Result<(), String> {
    // Reject re-entry; clear the flag on every exit path via the Drop guard.
    if DOWNLOAD_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("a model download is already in progress".to_string());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    let root = crate::storage::ariso_root()?;
    let models = crate::storage::models_dir(&root);
    std::fs::create_dir_all(&models).map_err(|e| format!("create models dir: {e}"))?;

    let bin = crate::transcribe::sidecar_path()?;
    let mut child = Command::new(&bin)
        .arg("download")
        .arg("--models")
        .arg(&models)
        .arg("--target")
        .arg(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;

    let stdout = child.stdout.take().ok_or("no stdout from sidecar")?;
    let stderr = child.stderr.take().ok_or("no stderr from sidecar")?;

    // Drain stderr concurrently so a large stderr burst can't fill the OS pipe
    // buffer and deadlock against our stdout read loop.
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut BufReader::new(stderr), &mut buf).await;
        buf
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(p) = serde_json::from_str::<ProgressLine>(&line) {
            if p.kind == "progress" {
                let _ = app.emit(ev_progress, p.fraction.unwrap_or(-1.0));
            }
        }
    }

    let exit = child.wait().await.map_err(|e| e.to_string())?;
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    if !exit.success() {
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        let _ = app.emit(ev_error, stderr.clone());
        return Err(format!("model download failed: {stderr}"));
    }

    if write_manifest_after {
        write_manifest(&root, &now_marker())?;
    }
    let _ = app.emit(ev_done, ());
    Ok(())
}

/// Download the STT models (ASR + diarizer) and write the readiness manifest.
#[tauri::command]
pub async fn download_local_stt(app: tauri::AppHandle) -> Result<(), String> {
    run_download(
        app,
        "stt",
        true,
        "model://stt/progress",
        "model://stt/done",
        "model://stt/error",
    )
    .await
}

/// Public CDN base for the notes LLM files (Cloudflare R2). The model is NOT
/// fetched via HuggingFace: the published `model.safetensors` is Xet-backed and
/// the Swift HF client can't download Xet, so we mirror plain files on R2 and
/// pull them directly.
const LLM_CDN_BASE: &str =
    "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/models/gemma-3-1b-it-qat-4bit";

/// The exact files the model loader needs. Doc/git files are omitted, and so is
/// `tokenizer.model` — Gemma ships a `tokenizer.json` fast tokenizer that the
/// loader uses, so the SentencePiece model is redundant (verified: the model
/// loads and generates without it).
const LLM_FILES: &[&str] = &[
    "config.json",
    "model.safetensors",
    "model.safetensors.index.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "added_tokens.json",
];

fn llm_fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        -1.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    }
}

/// Download the notes LLM directly from the app CDN into `<models>/llm/<name>/`,
/// emitting `model://llm/{progress,done,error}`. A `.complete` marker is written
/// only after every file finishes, so an interrupted download is never mistaken
/// for a ready model. Files already fully present are skipped (idempotent retry).
#[tauri::command]
pub async fn download_local_llm(app: tauri::AppHandle) -> Result<(), String> {
    if DOWNLOAD_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("a model download is already in progress".to_string());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    let root = crate::storage::ariso_root()?;
    let dir = llm_dir(&root);
    let app2 = app.clone();
    match download_llm_files(&dir, &move |f| {
        let _ = app2.emit("model://llm/progress", f);
    })
    .await
    {
        Ok(()) => {
            let _ = app.emit("model://llm/done", ());
            Ok(())
        }
        Err(e) => {
            let _ = app.emit("model://llm/error", e.clone());
            Err(e)
        }
    }
}

/// Download every `LLM_FILES` entry from the CDN into `dir`, reporting byte
/// progress (0.0–1.0) via `on_progress`. Decoupled from `AppHandle` so it can be
/// exercised by an integration test. Writes the `.complete` marker on success.
async fn download_llm_files(
    dir: &Path,
    on_progress: &(dyn Fn(f64) + Sync),
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("create llm dir: {e}"))?;
    let client = reqwest::Client::new();

    // 1) Size every file (HEAD) so progress is a true byte fraction.
    let mut sizes = Vec::with_capacity(LLM_FILES.len());
    for f in LLM_FILES {
        let url = format!("{LLM_CDN_BASE}/{f}");
        let resp = client
            .head(&url)
            .send()
            .await
            .map_err(|e| format!("head {f}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("head {f}: HTTP {}", resp.status()));
        }
        sizes.push(resp.content_length().unwrap_or(0));
    }
    let total: u64 = sizes.iter().sum();

    // 2) Download each file (skip already-complete), streaming with progress.
    let mut done: u64 = 0;
    for (i, f) in LLM_FILES.iter().enumerate() {
        let dest = dir.join(f);
        let expected = sizes[i];
        if let Ok(meta) = tokio::fs::metadata(&dest).await {
            if expected != 0 && meta.len() == expected {
                done += expected;
                on_progress(llm_fraction(done, total));
                continue;
            }
        }

        let url = format!("{LLM_CDN_BASE}/{f}");
        let mut resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("get {f}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("get {f}: HTTP {}", resp.status()));
        }

        let part = dir.join(format!("{f}.part"));
        let mut file = tokio::fs::File::create(&part)
            .await
            .map_err(|e| format!("create {f}.part: {e}"))?;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| format!("read {f}: {e}"))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("write {f}: {e}"))?;
            done += chunk.len() as u64;
            on_progress(llm_fraction(done, total));
        }
        file.flush().await.map_err(|e| format!("flush {f}: {e}"))?;
        drop(file);
        tokio::fs::rename(&part, &dest)
            .await
            .map_err(|e| format!("finalize {f}: {e}"))?;
    }

    // 3) Mark complete (readiness gate).
    tokio::fs::write(dir.join(".complete"), b"1")
        .await
        .map_err(|e| format!("write marker: {e}"))?;
    Ok(())
}

/// Opaque download marker. Only stored in manifest.json; never parsed for
/// logic (readiness is presence-based), so a `unix:<secs>` string suffices and
/// avoids pulling in a date crate.
fn now_marker() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_downloaded_then_ready_after_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert_eq!(status(root).state, "not_downloaded");
        assert!(!is_ready(root));

        write_manifest(root, "2026-06-02T00:00:00Z").unwrap();
        assert!(is_ready(root));
        let s = status(root);
        assert_eq!(s.state, "ready");
        assert_eq!(s.version.as_deref(), Some(MODEL_VERSION));
    }

    #[test]
    fn llm_ready_requires_complete_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(!llm_is_ready(root));
        assert_eq!(status(root).llm_ready, Some(false));

        // Files present but no marker → a partial download is NOT ready.
        let dir = llm_dir(root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        assert!(!llm_is_ready(root));

        // Marker written only after a full download → ready.
        std::fs::write(dir.join(".complete"), b"1").unwrap();
        assert!(llm_is_ready(root));
        assert_eq!(status(root).llm_ready, Some(true));
    }

    // Hits the network (downloads the full model from R2). Excluded from the
    // default run; invoke with `cargo test r2_download_smoke -- --ignored`.
    #[tokio::test]
    #[ignore = "network: downloads ~736MB from the R2 CDN"]
    async fn r2_download_smoke() {
        let tmp = tempfile::tempdir().unwrap();
        download_llm_files(tmp.path(), &|_| {}).await.unwrap();
        assert!(tmp.path().join(".complete").exists());
        for f in LLM_FILES {
            assert!(tmp.path().join(f).exists(), "missing {f}");
        }
        // The big weights file should be its full size.
        let st = std::fs::metadata(tmp.path().join("model.safetensors")).unwrap();
        assert!(st.len() > 700_000_000, "safetensors too small: {}", st.len());
    }
}
