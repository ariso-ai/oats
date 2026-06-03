use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::storage::MODEL_VERSION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
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

pub fn status(root: &Path) -> ModelStatus {
    match read_manifest(root) {
        Some(m) => ModelStatus { state: "ready".into(), version: Some(m.version) },
        None => ModelStatus { state: "not_downloaded".into(), version: None },
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

#[tauri::command]
pub async fn download_local_model(app: tauri::AppHandle) -> Result<(), String> {
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
                let _ = app.emit("model://progress", p.fraction.unwrap_or(-1.0));
            }
        }
    }

    let exit = child.wait().await.map_err(|e| e.to_string())?;
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    if !exit.success() {
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        let _ = app.emit("model://error", stderr.clone());
        return Err(format!("model download failed: {stderr}"));
    }

    write_manifest(&root, &now_marker())?;
    let _ = app.emit("model://done", ());
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
}
