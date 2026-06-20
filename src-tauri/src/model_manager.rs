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

/// Both on-device models are downloaded and ready to record with: the STT
/// (transcript) model AND the notes LLM. The Local backend gates recording on
/// this — see `commands::ensure_recording_allowed`, the tray, and the
/// mic-monitor auto-record path.
pub fn local_models_ready(root: &Path) -> bool {
    is_ready(root) && llm_is_ready(root)
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

/// Per-target download guards. STT writes `manifest.json` at the models root;
/// the LLM writes into `llm/<name>/` with its own `.complete` marker — disjoint
/// paths, so two *different* targets cannot race and may download in parallel.
/// Each flag still rejects a duplicate of its own target.
static STT_DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static LLM_DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// RAII guard over a download flag: sets it on `acquire`, clears it on drop
/// (every exit path). `acquire` returns `None` if the flag is already set.
struct DownloadGuard<'a>(&'a AtomicBool);

impl<'a> DownloadGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| DownloadGuard(flag))
    }
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

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
    guard_flag: &AtomicBool,
    target: &str,
    write_manifest_after: bool,
    ev_progress: &str,
    ev_done: &str,
    ev_error: &str,
) -> Result<(), String> {
    // Reject re-entry for THIS target; the guard clears the flag on drop.
    let _guard = DownloadGuard::acquire(guard_flag)
        .ok_or_else(|| "a model download is already in progress".to_string())?;

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
        &STT_DOWNLOAD_IN_PROGRESS,
        "stt",
        true,
        "model://stt/progress",
        "model://stt/done",
        "model://stt/error",
    )
    .await
}

/// Public base host for all app CDN assets (Cloudflare R2, r2.dev managed
/// domain). The desktop updater endpoint in `tauri.conf.json` is served from
/// this same host (`/desktop/latest.json`); keep them on one host. A macro
/// (not a `const`) so it can feed `concat!` below at compile time.
macro_rules! r2_base {
    () => {
        "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev"
    };
}

/// Public CDN base for the notes LLM files (Cloudflare R2). The model is NOT
/// fetched via HuggingFace: the published `model.safetensors` is Xet-backed and
/// the Swift HF client can't download Xet, so we mirror plain files on R2 and
/// pull them directly. The path is **version-pinned** (`/v1/`): the pinned
/// SHA-256s below assume these objects are immutable, so re-publishing the model
/// must use a new version segment rather than overwriting in place (otherwise
/// every client's verified download breaks until a new app ships). See
/// GHSA-9979-m4pv-g6f5.
const LLM_CDN_BASE: &str = concat!(r2_base!(), "/models/gemma-3-1b-it-qat-4bit/v1");

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

/// Pinned integrity metadata — `(sha256 lowercase hex, byte size)` — for every
/// published LLM file. The model is a fixed, versioned artifact, so each file's
/// digest and size are compile-time constants. Downloads are verified against
/// the digest before a `.part` file is promoted to its final name; a mismatch
/// (compromised R2 bucket, leaked write-creds, on-disk tampering, corruption) is
/// a hard error. The size is the disk-fill cap and the progress denominator, so
/// no network HEAD is trusted for sizing. This holds model downloads to the same
/// integrity bar the updater already enforces via minisign on this same R2 host
/// — see GHSA-9979-m4pv-g6f5 (CWE-494). To re-pin after a model bump:
/// `curl -fsSL <LLM_CDN_BASE>/<file> | shasum -a 256` and
/// `curl -sI <LLM_CDN_BASE>/<file>` for the Content-Length.
fn pinned(file: &str) -> Option<(&'static str, u64)> {
    Some(match file {
        "config.json" => ("eb080baebedaa32151a71988721a64f0be067fc6cd7e20ca16ba11231f822533", 1105),
        "model.safetensors" => (
            "b6010f6b03a83f973ca8708eb5784d5b0f80c0e7e9143dbb4c95d0eefe39c837",
            732_577_304,
        ),
        "model.safetensors.index.json" => (
            "b479eca1f14de16218fc5f45aa270d008944cd3f261f78e90f9b718c8857faef",
            50_542,
        ),
        "tokenizer.json" => (
            "4667f2089529e8e7657cfb6d1c19910ae71ff5f28aa7ab2ff2763330affad795",
            33_384_568,
        ),
        "tokenizer_config.json" => (
            "be9d72bdf5021aa82d67c3cc60cb0f8ddcc759d4d3f05eb129b9fcc345fc94b7",
            1_156_959,
        ),
        "special_tokens_map.json" => (
            "2f7b0adf4fb469770bb1490e3e35df87b1dc578246c5e7e6fc76ecf33213a397",
            662,
        ),
        "added_tokens.json" => ("50b2f405ba56a26d4913fd772089992252d7f942123cc0a034d96424221ba946", 35),
        _ => return None,
    })
}

/// Pinned SHA-256 (lowercase hex) for `file`, or `None` if we don't ship it.
fn expected_sha256(file: &str) -> Option<&'static str> {
    pinned(file).map(|(sha, _)| sha)
}

/// Pinned byte size for `file`, or `None` if we don't ship it.
fn expected_size(file: &str) -> Option<u64> {
    pinned(file).map(|(_, size)| size)
}

/// Stream a file through SHA-256, returning lowercase hex. Reads in 1 MiB chunks
/// so the ~700 MB weights file never loads fully into memory.
async fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Check a computed digest against the pinned one for `file`. A file with no
/// pinned digest is rejected outright (we never ship one), so verification can
/// never silently pass on an unexpected file.
fn verify_pinned(file: &str, actual_hex: &str) -> Result<(), String> {
    match expected_sha256(file) {
        Some(expected) if expected.eq_ignore_ascii_case(actual_hex) => Ok(()),
        Some(expected) => Err(format!(
            "integrity check failed for {file}: expected sha256 {expected}, got {actual_hex}"
        )),
        None => Err(format!("refusing unverified file {file}: no pinned sha256")),
    }
}

/// Download the notes LLM directly from the app CDN into `<models>/llm/<name>/`,
/// emitting `model://llm/{progress,done,error}`. A `.complete` marker is written
/// only after every file finishes, so an interrupted download is never mistaken
/// for a ready model. Each file is verified against a pinned SHA-256; an
/// already-present file is reused only if its digest still matches (idempotent
/// retry), otherwise it is re-downloaded.
#[tauri::command]
pub async fn download_local_llm(app: tauri::AppHandle) -> Result<(), String> {
    let _guard = DownloadGuard::acquire(&LLM_DOWNLOAD_IN_PROGRESS)
        .ok_or_else(|| "a model download is already in progress".to_string())?;

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
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("create llm dir: {e}"))?;
    // Clear any stale readiness marker before rewriting files: if a repair or
    // reinstall is interrupted, an old `.complete` must not keep `llm_is_ready`
    // true while a model file is partial. Re-written only after a full success.
    let marker = dir.join(".complete");
    let _ = tokio::fs::remove_file(&marker).await;
    // connect_timeout bounds a stalled handshake; read_timeout bounds the gap
    // *between* received bytes (not total duration), so the 700 MB weights file
    // can take as long as it needs as long as it keeps making progress.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("build http client: {e}"))?;

    // 1) Total comes from the compile-time pinned sizes — no network HEAD is
    //    trusted for sizing (a forgeable/zeroed Content-Length must never drive
    //    the cap or the progress bar). GHSA-9979-m4pv-g6f5.
    let total: u64 = LLM_FILES.iter().filter_map(|f| expected_size(f)).sum();

    // 2) Download each file, streaming with progress. Integrity is enforced by a
    //    pinned SHA-256 verified before the `.part` -> final rename; size alone
    //    is forgeable by whoever serves the bytes, so it is never the gate.
    let mut done: u64 = 0;
    for f in LLM_FILES {
        let dest = dir.join(f);
        // Every downloaded file must be pinned; refuse to fetch an unpinned one
        // rather than write unverifiable bytes to disk.
        let expected_len =
            expected_size(f).ok_or_else(|| format!("refusing unpinned file {f}"))?;

        // Resume: accept an already-present file only if its digest matches the
        // pin. A size match is not enough — re-verify or re-download.
        if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
            if let Ok(actual) = sha256_file(&dest).await
                && verify_pinned(f, &actual).is_ok()
            {
                done += expected_len;
                on_progress(llm_fraction(done, total));
                continue;
            }
            // Present but wrong or unreadable → discard and re-download.
            let _ = tokio::fs::remove_file(&dest).await;
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
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| format!("read {f}: {e}"))?
        {
            // Disk-fill guard: never write past the declared length. A correct
            // file ends exactly at expected_len, so this only trips on an origin
            // streaming more than it advertised.
            written += chunk.len() as u64;
            if written > expected_len {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(format!(
                    "{f} exceeds declared size {expected_len} bytes"
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("write {f}: {e}"))?;
            done += chunk.len() as u64;
            on_progress(llm_fraction(done, total));
        }
        file.flush().await.map_err(|e| format!("flush {f}: {e}"))?;
        drop(file);

        // Integrity gate: verify the streamed digest before promoting `.part`.
        // On mismatch, delete the partial file and fail — never expose unverified
        // bytes to the sidecar that mmaps and executes the weights.
        let actual = hex::encode(hasher.finalize());
        if let Err(e) = verify_pinned(f, &actual) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }
        tokio::fs::rename(&part, &dest)
            .await
            .map_err(|e| format!("finalize {f}: {e}"))?;
    }

    // 3) Mark complete (readiness gate).
    tokio::fs::write(&marker, b"1")
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

    #[test]
    fn local_models_ready_requires_both_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Neither marker present.
        assert!(!local_models_ready(root));

        // STT manifest only → not ready (LLM still missing).
        write_manifest(root, "2026-06-17T00:00:00Z").unwrap();
        assert!(!local_models_ready(root));

        // Add the LLM completion marker → both ready.
        let dir = llm_dir(root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".complete"), b"1").unwrap();
        assert!(local_models_ready(root));
    }

    #[test]
    fn local_models_ready_false_with_llm_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // LLM marker present but no STT manifest → not ready.
        let dir = llm_dir(root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".complete"), b"1").unwrap();
        assert!(!local_models_ready(root));
    }

    #[test]
    fn llm_cdn_base_uses_shared_r2_host() {
        // The desktop updater endpoint (tauri.conf.json) and the LLM mirror must
        // stay on the same R2 host. If this fails, the two URLs have drifted.
        assert!(
            LLM_CDN_BASE.starts_with(r2_base!()),
            "LLM_CDN_BASE must be served from the shared R2 host {}",
            r2_base!()
        );
        assert_eq!(r2_base!(), "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev");
    }

    #[test]
    fn every_llm_file_has_a_pinned_digest_and_size() {
        // Verification must not silently no-op: every file we download has to
        // carry a compile-time SHA-256 and a non-zero byte size to check against.
        for f in LLM_FILES {
            assert!(
                expected_sha256(f).is_some(),
                "no pinned sha256 for downloaded file {f}"
            );
            assert!(
                expected_size(f).is_some_and(|n| n > 0),
                "no pinned (non-zero) size for downloaded file {f}"
            );
        }
    }

    #[test]
    fn pinned_digest_present_for_safetensors() {
        assert_eq!(
            expected_sha256("model.safetensors"),
            Some("b6010f6b03a83f973ca8708eb5784d5b0f80c0e7e9143dbb4c95d0eefe39c837")
        );
        // A file we don't ship has no pinned digest.
        assert_eq!(expected_sha256("evil.bin"), None);
    }

    #[tokio::test]
    async fn sha256_file_hashes_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x");
        tokio::fs::write(&p, b"hello").await.unwrap();
        assert_eq!(
            sha256_file(&p).await.unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn verify_pinned_rejects_mismatch_and_accepts_match() {
        // Wrong bytes for a pinned file → error that names the file.
        let err = verify_pinned("model.safetensors", "00").unwrap_err();
        assert!(err.contains("model.safetensors"), "error should name file: {err}");
        // Correct digest → Ok.
        verify_pinned(
            "model.safetensors",
            "b6010f6b03a83f973ca8708eb5784d5b0f80c0e7e9143dbb4c95d0eefe39c837",
        )
        .unwrap();
    }

    #[test]
    fn download_guard_is_per_flag_and_releases_on_drop() {
        static A: AtomicBool = AtomicBool::new(false);
        static B: AtomicBool = AtomicBool::new(false);

        let held = DownloadGuard::acquire(&A).expect("first acquire on A");
        assert!(
            DownloadGuard::acquire(&A).is_none(),
            "same flag must reject a second acquire"
        );
        assert!(
            DownloadGuard::acquire(&B).is_some(),
            "a different flag must acquire independently"
        );

        drop(held);
        assert!(
            DownloadGuard::acquire(&A).is_some(),
            "flag must be free again after the guard drops"
        );
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
