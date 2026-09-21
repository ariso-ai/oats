use crate::speech_model::SpeechModelId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const WINDOWS_MODEL_LOCK_JSON: &str = include_str!("../ariso-stt/shared/windows-models.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsModelLock {
    schema_version: u32,
    cdn_base: String,
    speech: Vec<LockedModelBundle>,
    notes: Vec<LockedModelBundle>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedModelBundle {
    folder: String,
    prefix: String,
    install_path: String,
    manifest_sha256: String,
    files: Vec<String>,
}

static WINDOWS_MODEL_LOCK: LazyLock<WindowsModelLock> = LazyLock::new(|| {
    let lock: WindowsModelLock =
        serde_json::from_str(WINDOWS_MODEL_LOCK_JSON).expect("valid embedded Windows model lock");
    assert_eq!(
        lock.schema_version, 1,
        "supported Windows model lock schema"
    );
    lock
});

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// Whether the *selected* speech model is ready. See `speech` below for
    /// per-model readiness.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the on-device notes LLM (gemma) has been downloaded. Reported
    /// separately from `state` so the Settings window can show the LLM's own
    /// download status alongside the overall model status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_ready: Option<bool>,
    /// Per speech model readiness, for every model available on this platform.
    pub speech: Vec<SpeechModelStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelStatus {
    pub id: SpeechModelId,
    pub ready: bool,
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
/// The host writes this marker only after every model file downloads and
/// verifies. Windows includes the immutable bundle revisions in the marker so
/// an app update that changes model paths invalidates an older installation.
/// Individual files are not re-hashed on every status check; deleting files
/// manually after installation therefore surfaces as a transcription failure.
pub fn is_ready(root: &Path) -> bool {
    read_manifest(root).is_some_and(|manifest| manifest.version == stt_model_version())
}

pub fn read_manifest(root: &Path) -> Option<ModelManifest> {
    let bytes = std::fs::read(manifest_path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_manifest(root: &Path, downloaded_at: &str) -> Result<(), String> {
    let dir = crate::storage::models_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create models dir: {e}"))?;
    let manifest = ModelManifest {
        version: stt_model_version(),
        downloaded_at: downloaded_at.to_string(),
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(&manifest_path(root), json.as_bytes())
}

/// Stable macOS recording/model identity retained for existing manifests. The
/// exact CoreML and diarization revisions are pinned separately by the commit
/// prefixes returned by `macos_stt_bundles()`.
const MACOS_STT_MODEL_VERSION: &str = "parakeet-tdt-0.6b-v3";

/// Both platforms install their runtime-specific Gemma files under this shared
/// logical model directory. The contents differ (MLX/safetensors on macOS,
/// GGUF on Windows), but callers never need a platform-specific model path.
const LLM_MODEL_NAME: &str = "gemma-3-1b-it-qat-4bit";

/// Existing macOS installs write `1` into `.complete`; treating it as the
/// macOS bundle identity preserves those downloads while Windows starts with a
/// stronger bundle-derived identity from day one.
const MACOS_LLM_MARKER_VERSION: &str = "1";

fn llm_dir(root: &Path) -> std::path::PathBuf {
    crate::storage::models_dir(root)
        .join("llm")
        .join(LLM_MODEL_NAME)
}

/// Join immutable bundle locations into one readiness identity. Folder names
/// disambiguate independently versioned components that may share a tag such as
/// `v1`; changing any folder or prefix invalidates the aggregate identity.
fn bundle_version(bundles: &[ModelBundle]) -> String {
    bundles
        .iter()
        .map(|bundle| format!("{}@{}", bundle.folder, bundle.prefix))
        .collect::<Vec<_>>()
        .join("+")
}

/// Produces the readiness identity recorded in the shared STT manifest and each
/// completed recording. macOS retains its established product-level identity;
/// its exact CDN URLs use the upstream prefixes from `macos_stt_bundles()`.
/// Windows has no legacy installs, so its identity joins every `folder@tag`
/// component and a change to either ASR or diarization requires a fresh bundle.
pub(crate) fn stt_model_version() -> String {
    if cfg!(target_os = "windows") {
        bundle_version(&windows_stt_bundles())
    } else {
        MACOS_STT_MODEL_VERSION.to_string()
    }
}

/// Version expected in the shared Gemma completion marker. Both targets use the
/// same model family and local path, while the value identifies the platform's
/// runtime representation: the existing macOS R2 mirror revision or the exact
/// Windows GGUF data-bundle tag.
fn llm_model_version() -> String {
    if cfg!(target_os = "windows") {
        bundle_version(&windows_llm_bundles())
    } else {
        MACOS_LLM_MARKER_VERSION.to_string()
    }
}

/// The notes readiness marker has one location on every desktop platform.
/// Runtime-specific artifact versions live in its contents, not in local paths.
fn llm_marker_path(root: &Path) -> PathBuf {
    llm_dir(root).join(".complete")
}

/// Readiness for the notes LLM requires the current marker identity, not mere
/// presence. This rejects interrupted downloads and stale Windows bundles
/// without carrying compatibility branches for layouts that never shipped.
pub fn llm_is_ready(root: &Path) -> bool {
    let expected = llm_model_version();
    std::fs::read_to_string(llm_marker_path(root))
        .is_ok_and(|version| version.trim() == expected.as_str())
}

/// Both on-device models are downloaded and ready to record with: the
/// *selected* speech model AND the notes LLM. The Local backend gates
/// recording on this — see `commands::ensure_recording_allowed`, the tray, and
/// the mic-monitor auto-record path.
pub fn local_models_ready(root: &Path) -> bool {
    speech_is_ready(root, crate::speech_model::selected()) && llm_is_ready(root)
}

pub fn status(root: &Path) -> ModelStatus {
    let llm_ready = Some(llm_is_ready(root));
    let selected = crate::speech_model::selected();
    let speech = crate::speech_model::available()
        .into_iter()
        .map(|id| SpeechModelStatus {
            id,
            ready: speech_is_ready(root, id),
        })
        .collect();
    if speech_is_ready(root, selected) {
        ModelStatus {
            state: "ready".into(),
            version: Some(match selected {
                SpeechModelId::Parakeet => read_manifest(root)
                    .map(|m| m.version)
                    .unwrap_or_else(stt_model_version),
                _ => speech_model_version(selected),
            }),
            llm_ready,
            speech,
        }
    } else {
        ModelStatus {
            state: "not_downloaded".into(),
            version: None,
            llm_ready,
            speech,
        }
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

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

/// Presents model readiness to the webview without asking unsupported targets to
/// probe platform-specific layouts. Actual downloads remain separate commands so
/// merely opening Settings never initiates network activity.
#[tauri::command]
pub fn local_model_status() -> Result<ModelStatus, String> {
    if !(cfg!(target_os = "macos") || cfg!(target_os = "windows")) {
        return Ok(ModelStatus {
            state: "unsupported".into(),
            version: None,
            llm_ready: Some(false),
            speech: vec![],
        });
    }
    let root = crate::storage::ariso_root()?;
    Ok(status(&root))
}

/// Progress payload for `model://stt/progress`: which speech model is
/// downloading and how far along it is, so a download of a non-selected model
/// (e.g. queued from the model picker) doesn't get misread as progress on the
/// selected one.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SttProgress {
    model: SpeechModelId,
    fraction: f64,
}

/// Download and verify one speech model's bundles from the R2 mirror, then
/// write its readiness marker. `model` defaults to the selected speech model.
/// See `download_model_bundles` for the integrity model. Every speech download
/// shares one guard: models can share on-disk directories (the diarizer), and
/// two concurrent downloads would race on its `.part` files.
#[tauri::command]
pub async fn download_local_stt(
    app: tauri::AppHandle,
    model: Option<SpeechModelId>,
) -> Result<(), String> {
    if !(cfg!(target_os = "macos") || cfg!(target_os = "windows")) {
        let msg = "Local STT is not supported on this platform".to_string();
        let _ = app.emit("model://stt/error", msg.clone());
        return Err(msg);
    }
    let model = model.unwrap_or_else(crate::speech_model::selected);
    if !model.is_available() {
        let msg = "That speech model isn't available on this platform".to_string();
        let _ = app.emit("model://stt/error", msg.clone());
        return Err(msg);
    }

    let _guard = DownloadGuard::acquire(&STT_DOWNLOAD_IN_PROGRESS)
        .ok_or_else(|| "a model download is already in progress".to_string())?;

    let root = crate::storage::ariso_root()?;
    let models = crate::storage::models_dir(&root);
    // Clear any stale readiness marker before (re)downloading: an interrupted
    // run must not leave the marker claiming this model is ready. It is
    // rewritten only after every file downloads and verifies.
    if let Err(error) = clear_speech_marker(&root, model) {
        return Err(format!("invalidate STT readiness marker: {error}"));
    }

    let app2 = app.clone();
    let cdn_base = if cfg!(target_os = "windows") {
        WINDOWS_MODEL_LOCK.cdn_base.clone()
    } else {
        MODELS_CDN_BASE.to_string()
    };
    let result = download_model_bundles(
        &cdn_base,
        &models,
        &speech_bundles(model),
        &move |fraction| {
            let _ = app2.emit("model://stt/progress", SttProgress { model, fraction });
        },
    )
    .await
    .and_then(|()| write_speech_marker(&root, model));

    match result {
        Ok(()) => {
            let _ = app.emit("model://stt/done", model);
            Ok(())
        }
        Err(e) => {
            let _ = app.emit("model://stt/error", e.clone());
            Err(e)
        }
    }
}

/// Public base host for all app CDN assets (Cloudflare R2, r2.dev managed
/// domain). The desktop updater endpoints in `tauri.conf.json` are served from
/// this same host (`/desktop/latest-{target}-{arch}.json`); keep them on one
/// host. A macro (not a `const`) so it can feed `concat!` below at compile time.
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
        "config.json" => (
            "eb080baebedaa32151a71988721a64f0be067fc6cd7e20ca16ba11231f822533",
            1105,
        ),
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
        "added_tokens.json" => (
            "50b2f405ba56a26d4913fd772089992252d7f942123cc0a034d96424221ba946",
            35,
        ),
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

/// Download the platform's notes assets into the shared `<models>/llm/<name>/`
/// directory, emitting `model://llm/{progress,done,error}`. macOS downloads its
/// pinned MLX files directly; Windows installs a hash-pinned GGUF data bundle
/// while llama.cpp ships as an installer resource. Both write the same
/// versioned readiness marker only after success.
#[tauri::command]
pub async fn download_local_llm(app: tauri::AppHandle) -> Result<(), String> {
    if !(cfg!(target_os = "macos") || cfg!(target_os = "windows")) {
        let msg = "Local LLM is not supported on this platform".to_string();
        let _ = app.emit("model://llm/error", msg.clone());
        return Err(msg);
    }

    let _guard = DownloadGuard::acquire(&LLM_DOWNLOAD_IN_PROGRESS)
        .ok_or_else(|| "a model download is already in progress".to_string())?;

    let root = crate::storage::ariso_root()?;
    let dir = llm_dir(&root);
    let app2 = app.clone();
    let result = if cfg!(target_os = "windows") {
        let models = crate::storage::models_dir(&root);
        let marker = llm_marker_path(&root);
        if let Err(error) = tokio::fs::remove_file(&marker).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!("invalidate LLM readiness marker: {error}"));
        }
        let bundles = windows_llm_bundles();
        match download_model_bundles(&WINDOWS_MODEL_LOCK.cdn_base, &models, &bundles, &move |f| {
            let _ = app2.emit("model://llm/progress", f);
        })
        .await
        {
            Ok(()) => write_llm_marker(&marker).await,
            Err(error) => Err(error),
        }
    } else {
        download_llm_files(&dir, &move |f| {
            let _ = app2.emit("model://llm/progress", f);
        })
        .await
    };

    match result {
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

/// Publish readiness through one marker format after all model assets verify.
/// Keeping this write shared prevents the macOS and Windows download paths from
/// drifting into different completion semantics.
async fn write_llm_marker(marker: &Path) -> Result<(), String> {
    let marker_dir = marker
        .parent()
        .ok_or_else(|| "invalid llm marker path".to_string())?;
    tokio::fs::create_dir_all(marker_dir)
        .await
        .map_err(|e| format!("create llm marker dir: {e}"))?;
    tokio::fs::write(marker, llm_model_version())
        .await
        .map_err(|e| format!("write llm marker: {e}"))
}

/// Download every `LLM_FILES` entry from the CDN into `dir`, reporting byte
/// progress (0.0–1.0) via `on_progress`. Decoupled from `AppHandle` so it can be
/// exercised by an integration test. Writes the `.complete` marker on success.
async fn download_llm_files(dir: &Path, on_progress: &(dyn Fn(f64) + Sync)) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("create llm dir: {e}"))?;
    // Clear any stale readiness marker before rewriting files: if a repair or
    // reinstall is interrupted, an old `.complete` must not keep `llm_is_ready`
    // true while a model file is partial. Re-written only after a full success.
    let marker = dir.join(".complete");
    if let Err(error) = tokio::fs::remove_file(&marker).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("invalidate LLM readiness marker: {error}"));
    }
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
        let expected_len = expected_size(f).ok_or_else(|| format!("refusing unpinned file {f}"))?;

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
            tokio::fs::remove_file(&dest)
                .await
                .map_err(|error| format!("remove invalid {}: {error}", dest.display()))?;
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
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("read {f}: {e}"))? {
            // Disk-fill guard: never write past the declared length. A correct
            // file ends exactly at expected_len, so this only trips on an origin
            // streaming more than it advertised.
            written += chunk.len() as u64;
            if written > expected_len {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(format!("{f} exceeds declared size {expected_len} bytes"));
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
    write_llm_marker(&marker).await?;
    Ok(())
}

/// A platform-native model bundle at an immutable R2 prefix. macOS STT prefixes
/// are upstream commit hashes; Windows prefixes are explicit release tags tied
/// to pinned upstream model revisions. `bundle_version` joins folder + prefix
/// when Windows needs one aggregate readiness identity. A model bump always
/// publishes a new prefix and pin rather than overwriting one.
#[derive(Clone, Debug)]
struct ModelBundle {
    /// R2 path below the shared `models/` base.
    folder: String,
    /// Immutable revision segment below `folder`.
    prefix: String,
    /// Relative installation directory under the local models root.
    install_path: String,
    /// SHA-256 (lowercase hex) of the prefix's `SHA256SUMS` over its raw bytes —
    /// the single pinned trust anchor. A tampered file list can't match it, and
    /// every model file is then verified against an entry in that list.
    manifest_sha256: String,
    /// Optional allowlist of model data files. Executable runtimes are installer
    /// resources and never enter the post-install model download path.
    files: Option<Vec<String>>,
}

/// The FluidAudio diarizer every macOS speech model shares — Parakeet and
/// Qwen3-ASR both assign speakers with it, so it lives in its own function
/// rather than being duplicated across bundle lists.
fn macos_diarizer_bundle() -> ModelBundle {
    ModelBundle {
        folder: "speaker-diarization".into(),
        prefix: "1ed7a662fdc7".into(),
        install_path: "speaker-diarization".into(),
        manifest_sha256: "bc9cf65e567d862fa30aea1e71831d7c1d2dddcf58c22e2f90aaac28dc8baa74".into(),
        files: None,
    }
}

/// Retains the existing CoreML/FluidAudio installation layout expected by the
/// Swift sidecar. The generic downloader can therefore serve both platforms
/// without forcing a shared on-disk model representation.
fn macos_stt_bundles() -> Vec<ModelBundle> {
    vec![
        ModelBundle {
            folder: "parakeet-tdt-0.6b-v3".into(),
            prefix: "aed027400592".into(),
            install_path: "parakeet-tdt-0.6b-v3".into(),
            manifest_sha256: "58ca342f4648ed43233f627200d65a605fc6d97807bc300484bfc01b1cb2aa30"
                .into(),
            files: None,
        },
        macos_diarizer_bundle(),
    ]
}

const QWEN3_ASR_DIR: &str = "qwen3-asr-0.6b-4bit";
const QWEN3_ALIGNER_DIR: &str = "qwen3-forcedaligner-0.6b-4bit";

/// Qwen3-ASR transcribes, the forced aligner times each word, and the shared
/// FluidAudio diarizer assigns speakers — all three must be present.
fn macos_qwen3_bundles() -> Vec<ModelBundle> {
    vec![
        ModelBundle {
            folder: QWEN3_ASR_DIR.into(),
            prefix: "313d85018176".into(),
            install_path: QWEN3_ASR_DIR.into(),
            manifest_sha256: "e67be63dffa605fe9332adebeb34a4e53096c0c9a6540ef24ae53a1cb7fef5a3"
                .into(),
            files: None,
        },
        ModelBundle {
            folder: QWEN3_ALIGNER_DIR.into(),
            prefix: "2f652af86ae0".into(),
            install_path: QWEN3_ALIGNER_DIR.into(),
            manifest_sha256: "930c0dbb18b0ac19bb2df027f03436062487d91ec9c1ea97125d9cb30db82cdf"
                .into(),
            files: None,
        },
        macos_diarizer_bundle(),
    ]
}

/// This speech model's install bundles on the current platform. Windows ships
/// only Parakeet; Qwen3-ASR is macOS-only (see `SpeechModelId::is_available`).
fn speech_bundles(model: SpeechModelId) -> Vec<ModelBundle> {
    match model {
        SpeechModelId::Parakeet if cfg!(target_os = "windows") => windows_stt_bundles(),
        SpeechModelId::Parakeet => macos_stt_bundles(),
        SpeechModelId::Qwen3Asr => macos_qwen3_bundles(),
    }
}

/// Public CDN base for the STT model mirror (same R2 host as the LLM + updater).
const MODELS_CDN_BASE: &str = concat!(r2_base!(), "/models");

fn locked_bundles(definitions: &[LockedModelBundle]) -> Vec<ModelBundle> {
    definitions
        .iter()
        .map(|definition| ModelBundle {
            folder: definition.folder.clone(),
            prefix: definition.prefix.clone(),
            install_path: definition.install_path.clone(),
            manifest_sha256: definition.manifest_sha256.clone(),
            files: Some(definition.files.clone()),
        })
        .collect()
}

fn windows_stt_bundles() -> Vec<ModelBundle> {
    locked_bundles(&WINDOWS_MODEL_LOCK.speech)
}

fn windows_llm_bundles() -> Vec<ModelBundle> {
    locked_bundles(&WINDOWS_MODEL_LOCK.notes)
}

/// Per-file disk-fill backstop for bundle downloads. The authoritative integrity gate
/// is the per-file SHA-256 from the (hash-pinned) manifest; this only bounds how
/// many bytes a compromised origin could write before that hash fails — the
/// SHA256SUMS manifest, unlike the LLM pins, carries no per-file size to cap
/// against. Set well above the largest real file (~440 MiB encoder weights).
const MODEL_MAX_FILE_BYTES: u64 = 1 << 30; // 1 GiB

/// One `<sha256>  <relpath>` row of a SHA256SUMS manifest.
#[derive(Clone, Debug)]
struct ManifestEntry {
    sha256: String,
    path: String,
}

/// SHA-256 (lowercase hex) of an in-memory buffer — used for the manifest itself.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Parse a SHA256SUMS manifest (coreutils format: 64 hex chars, a space, a
/// space|`*` marker, then the path). Rejects malformed lines, an empty manifest,
/// and any path that could escape the model dir.
fn parse_sha256sums(text: &str) -> Result<Vec<ManifestEntry>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() < 67 {
            return Err(format!("line {n}: malformed"));
        }
        let (hash, rest) = line.split_at(64);
        if !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("line {n}: invalid hash"));
        }
        let sep = rest.as_bytes();
        if sep[0] != b' ' || (sep[1] != b' ' && sep[1] != b'*') {
            return Err(format!("line {n}: invalid separator"));
        }
        let path = &rest[2..];
        sanitize_rel_path(path).map_err(|e| format!("line {n}: {e}"))?;
        out.push(ManifestEntry {
            sha256: hash.to_ascii_lowercase(),
            path: path.to_string(),
        });
    }
    if out.is_empty() {
        return Err("manifest is empty".into());
    }
    Ok(out)
}

fn select_manifest_entries(
    entries: Vec<ManifestEntry>,
    required_files: Option<&[String]>,
) -> Result<Vec<ManifestEntry>, String> {
    let Some(required_files) = required_files else {
        return Ok(entries);
    };

    let mut selected = Vec::with_capacity(required_files.len());
    let mut seen = std::collections::HashSet::new();
    for required in required_files {
        sanitize_rel_path(required)?;
        if !seen.insert(required) {
            return Err(format!("duplicate required model file {required:?}"));
        }
        let entry = entries
            .iter()
            .find(|entry| entry.path == *required)
            .ok_or_else(|| format!("required model file missing from manifest: {required}"))?;
        selected.push(entry.clone());
    }
    if selected.is_empty() {
        return Err("bundle selects no model files".into());
    }
    Ok(selected)
}

/// Validate a manifest path is a safe relative path (no absolute root, `.`, `..`,
/// or empty components) and return it as a `PathBuf`. Defense-in-depth against
/// traversal even though the manifest is hash-pinned.
fn sanitize_rel_path(path: &str) -> Result<PathBuf, String> {
    use std::path::Component;
    if path.is_empty() {
        return Err("empty path".into());
    }
    let mut safe = PathBuf::new();
    for comp in PathBuf::from(path).components() {
        match comp {
            Component::Normal(c) => safe.push(c),
            _ => return Err(format!("unsafe path: {path:?}")),
        }
    }
    Ok(safe)
}

/// Append `.part` to a destination path (`weight.bin` -> `weight.bin.part`).
fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Download and verify model bundles into their platform installation paths,
/// reporting a 0.0–1.0 per-file progress fraction via `on_progress`. Per bundle: fetch
/// `SHA256SUMS` and check it against the pinned hash (the trust anchor), then
/// download each listed file (skipping any already present whose hash matches) and
/// verify it against the manifest before the atomic rename. Like the LLM path it
/// trusts no network-provided size — the per-file SHA-256 is the gate and a fixed
/// cap bounds disk-fill (GHSA-9979-m4pv-g6f5). Decoupled from `AppHandle` for
/// testing; does NOT write the readiness manifest — the caller does that on full
/// success.
async fn download_model_bundles(
    base: &str,
    models_dir: &Path,
    bundles: &[ModelBundle],
    on_progress: &(dyn Fn(f64) + Sync),
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("build http client: {e}"))?;

    struct Pending {
        url: String,
        dest: PathBuf,
        sha256: String,
    }
    let mut pending: Vec<Pending> = Vec::new();

    // 1) Per model: fetch the manifest, verify it against the pinned hash, parse.
    for m in bundles {
        let prefix = format!("{base}/{}/{}", m.folder, m.prefix);
        let resp = client
            .get(format!("{prefix}/SHA256SUMS"))
            .send()
            .await
            .map_err(|e| format!("get manifest {}: {e}", m.folder))?;
        if !resp.status().is_success() {
            return Err(format!("get manifest {}: HTTP {}", m.folder, resp.status()));
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| format!("read manifest {}: {e}", m.folder))?;
        let got = sha256_hex(&body);
        if !got.eq_ignore_ascii_case(&m.manifest_sha256) {
            return Err(format!(
                "manifest integrity check failed for {}: expected {}, got {got}",
                m.folder, m.manifest_sha256
            ));
        }
        let text = std::str::from_utf8(&body)
            .map_err(|_| format!("manifest {} is not UTF-8", m.folder))?;
        let entries = parse_sha256sums(text).map_err(|e| format!("manifest {}: {e}", m.folder))?;
        let entries = select_manifest_entries(entries, m.files.as_deref())
            .map_err(|e| format!("manifest {}: {e}", m.folder))?;
        for e in entries {
            let rel = sanitize_rel_path(&e.path)?;
            pending.push(Pending {
                url: format!("{prefix}/{}", e.path),
                dest: models_dir.join(&m.install_path).join(rel),
                sha256: e.sha256,
            });
        }
    }

    // 2) Download (skip already-verified files) and verify each before renaming.
    //    Progress is per-file: the manifest carries no sizes, and a network
    //    Content-Length must not drive the bar (GHSA-9979-m4pv-g6f5).
    let total = pending.len() as f64;
    for (i, p) in pending.iter().enumerate() {
        if let Some(parent) = p.dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
        }

        // Resume: keep an already-present file only if its digest matches the pin.
        if tokio::fs::try_exists(&p.dest).await.unwrap_or(false) {
            if sha256_file(&p.dest)
                .await
                .is_ok_and(|h| h.eq_ignore_ascii_case(&p.sha256))
            {
                on_progress(((i + 1) as f64 / total).clamp(0.0, 1.0));
                continue;
            }
            let _ = tokio::fs::remove_file(&p.dest).await;
        }

        let mut resp = client
            .get(&p.url)
            .send()
            .await
            .map_err(|e| format!("get {}: {e}", p.url))?;
        if !resp.status().is_success() {
            return Err(format!("get {}: HTTP {}", p.url, resp.status()));
        }

        let part = part_path(&p.dest);
        let mut file = tokio::fs::File::create(&part)
            .await
            .map_err(|e| format!("create {}: {e}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| format!("read {}: {e}", p.url))?
        {
            // Disk-fill backstop: the SHA-256 below is the real gate, but bound how
            // much a misbehaving origin can write before we reach it.
            written += chunk.len() as u64;
            if written > MODEL_MAX_FILE_BYTES {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(format!(
                    "{} exceeds the {MODEL_MAX_FILE_BYTES}-byte cap",
                    p.dest.display()
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("write {}: {e}", part.display()))?;
        }
        file.flush()
            .await
            .map_err(|e| format!("flush {}: {e}", part.display()))?;
        drop(file);

        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(&p.sha256) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(format!(
                "integrity check failed for {}: expected {}, got {got}",
                p.dest.display(),
                p.sha256
            ));
        }
        tokio::fs::rename(&part, &p.dest)
            .await
            .map_err(|e| format!("finalize {}: {e}", p.dest.display()))?;
        on_progress(((i + 1) as f64 / total).clamp(0.0, 1.0));
    }

    Ok(())
}

// --- Size on disk and removal ----------------------------------------------

/// Which installed local model a size or delete request names. A closed enum,
/// never a path or free-text id, so a webview cannot point these commands at a
/// directory of its own choosing (`oats-security` surface #2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LocalModelKind {
    Notes,
    Speech,
}

/// Bytes each local model occupies, or `None` for one that is not installed.
/// `speech` covers every speech model available on this platform, keyed by id.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSizes {
    pub notes: Option<u64>,
    pub speech: std::collections::BTreeMap<SpeechModelId, Option<u64>>,
}

/// Install directories of one speech model's bundles on this platform.
fn speech_dirs(root: &Path, model: SpeechModelId) -> Vec<PathBuf> {
    let models = crate::storage::models_dir(root);
    speech_bundles(model)
        .iter()
        .map(|bundle| models.join(&bundle.install_path))
        .collect()
}

/// Readiness marker of a speech model that doesn't use the legacy root
/// `manifest.json`. Parakeet keeps `manifest.json` so existing installs stay
/// ready; newer models write `<asr dir>/.complete` holding their bundle identity.
fn speech_marker_path(root: &Path, model: SpeechModelId) -> Option<PathBuf> {
    match model {
        SpeechModelId::Parakeet => None,
        SpeechModelId::Qwen3Asr => Some(
            crate::storage::models_dir(root)
                .join(QWEN3_ASR_DIR)
                .join(".complete"),
        ),
    }
}

/// The readiness identity for one speech model. Parakeet keeps its existing
/// product-level version string (`stt_model_version`) so old installs stay
/// ready; every other model's identity is its bundle set's folder@prefix join.
pub fn speech_model_version(model: SpeechModelId) -> String {
    match model {
        SpeechModelId::Parakeet => stt_model_version(),
        _ => bundle_version(&speech_bundles(model)),
    }
}

/// Write this model's readiness marker after every one of its bundle files
/// downloads and verifies.
fn write_speech_marker(root: &Path, model: SpeechModelId) -> Result<(), String> {
    match speech_marker_path(root, model) {
        None => write_manifest(root, &now_marker()),
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("create {}: {e}", parent.display()))?;
            }
            std::fs::write(&path, speech_model_version(model))
                .map_err(|e| format!("write {}: {e}", path.display()))
        }
    }
}

/// Invalidate this model's readiness marker before a (re)download so an
/// interrupted run never leaves it claiming to be ready.
fn clear_speech_marker(root: &Path, model: SpeechModelId) -> Result<(), String> {
    let path = speech_marker_path(root, model).unwrap_or_else(|| manifest_path(root));
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("remove {}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Whether `model` is downloaded and ready to transcribe with: available on
/// this platform, its readiness marker matches its current bundle identity,
/// and every one of its install directories exists.
pub fn speech_is_ready(root: &Path, model: SpeechModelId) -> bool {
    if !model.is_available() {
        return false;
    }
    match speech_marker_path(root, model) {
        None => is_ready(root),
        Some(marker) => {
            std::fs::read_to_string(marker).is_ok_and(|v| v.trim() == speech_model_version(model))
                && speech_dirs(root, model).iter().all(|d| d.exists())
        }
    }
}

/// What this speech model occupies on disk, or `None` when none of its
/// directories exist — which is how "not installed" is distinguished from
/// "installed but empty", so the UI can show a dash rather than `0 MB`. A
/// directory shared with another installed speech model (the diarizer) is
/// still counted here: each ready model's own footprint includes what it needs.
pub fn speech_model_size(root: &Path, model: SpeechModelId) -> Option<u64> {
    let dirs = speech_dirs(root, model);
    if !dirs.iter().any(|d| d.exists()) {
        return None;
    }
    Some(dirs.iter().map(|d| dir_size(d)).sum())
}

/// Remove one speech model's files and readiness marker, so the next
/// recording attempt with it re-downloads instead of failing on a
/// half-present model. A directory another *installed* speech model also uses
/// (the shared diarizer) is kept rather than deleted out from under it.
/// Idempotent: removing a model that is not installed succeeds and does
/// nothing.
pub fn delete_speech_model(root: &Path, model: SpeechModelId) -> Result<(), String> {
    let keep: std::collections::HashSet<PathBuf> = SpeechModelId::ALL
        .into_iter()
        .filter(|other| *other != model && speech_is_ready(root, *other))
        .flat_map(|other| speech_dirs(root, other))
        .collect();
    clear_speech_marker(root, model)?;
    for dir in speech_dirs(root, model) {
        if dir.exists() && !keep.contains(&dir) {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// Total bytes under `dir`. Symlinks are counted as neither file nor directory:
/// following one could walk (or, for a caller that deletes, reach) somewhere
/// outside the models tree entirely. Unreadable entries are skipped rather than
/// failing the walk — a displayed size is advisory, not a correctness gate.
fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        // `DirEntry::metadata` does not traverse symlinks.
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()));
        } else if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// What the notes LLM occupies on disk, or `None` when its directory doesn't
/// exist — which is how "not installed" is distinguished from "installed but
/// empty", so the UI can show a dash rather than `0 MB`.
fn notes_size(root: &Path) -> Option<u64> {
    let dir = llm_dir(root);
    if !dir.exists() {
        return None;
    }
    Some(dir_size(&dir))
}

/// Remove the notes LLM's files, readiness marker included, so the next
/// recording attempt re-downloads instead of failing on a half-present model.
/// Idempotent: removing a model that is not installed succeeds and does
/// nothing.
fn delete_notes(root: &Path) -> Result<(), String> {
    let dir = llm_dir(root);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Sizes for the Settings model list. Reads the filesystem only — never the
/// network — so opening Settings in Local mode stays offline.
#[tauri::command]
pub fn local_model_sizes() -> Result<ModelSizes, String> {
    let root = crate::storage::ariso_root()?;
    Ok(ModelSizes {
        notes: notes_size(&root),
        speech: crate::speech_model::available()
            .into_iter()
            .map(|m| (m, speech_model_size(&root, m)))
            .collect(),
    })
}

/// Delete one local model's files. Refused mid-recording (that session still
/// needs its models to transcribe and write notes) and while the same model is
/// downloading — the download guard doubles as the mutual-exclusion lock, so a
/// delete can never race a half-written install. `speech_model` names which
/// speech model to remove when `kind` is `Speech`; `None` means the selected one.
#[tauri::command]
pub fn delete_local_model(
    app: tauri::AppHandle,
    kind: LocalModelKind,
    speech_model: Option<SpeechModelId>,
) -> Result<(), String> {
    use tauri::Manager as _;
    if app
        .state::<crate::recording_state::RecordingState>()
        .is_active()
    {
        return Err("Can't remove a model while a recording is in progress.".into());
    }
    let speech_model = speech_model.unwrap_or_else(crate::speech_model::selected);
    if kind == LocalModelKind::Speech && !speech_model.is_available() {
        return Err("That speech model isn't available on this platform".into());
    }
    let flag = match kind {
        LocalModelKind::Notes => &LLM_DOWNLOAD_IN_PROGRESS,
        LocalModelKind::Speech => &STT_DOWNLOAD_IN_PROGRESS,
    };
    let _guard = DownloadGuard::acquire(flag)
        .ok_or_else(|| "That model is still downloading.".to_string())?;
    let root = crate::storage::ariso_root()?;
    match kind {
        LocalModelKind::Notes => delete_notes(&root),
        LocalModelKind::Speech => delete_speech_model(&root, speech_model),
    }
}

/// Opaque download timestamp stored in `manifest.json`; a `unix:<secs>` string
/// suffices and avoids pulling in a date crate.
fn now_marker() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

/// Test-only: write the STT readiness manifest directly, bypassing the network
/// download path. `transcribe.rs`'s tests use this instead of duplicating
/// `manifest_path`/`stt_model_version` (both private to this module).
#[cfg(test)]
pub(crate) fn mark_stt_ready_for_test(root: &Path) {
    write_manifest(root, "2026-01-01T00:00:00Z").expect("write STT manifest for test");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `bytes` worth of file at `path`, creating parents.
    fn write_bytes(path: &std::path::Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn install_notes(root: &Path, bytes: usize) {
        write_bytes(&llm_dir(root).join("model.safetensors"), bytes);
        std::fs::write(llm_marker_path(root), llm_model_version()).unwrap();
    }

    fn install_parakeet(root: &Path, bytes_each: usize) {
        for bundle in speech_bundles(SpeechModelId::Parakeet) {
            write_bytes(
                &crate::storage::models_dir(root)
                    .join(&bundle.install_path)
                    .join("w.bin"),
                bytes_each,
            );
        }
        write_manifest(root, "unix:1").unwrap();
    }

    #[cfg(target_os = "macos")]
    fn install_qwen3(root: &Path, bytes_each: usize) {
        for bundle in speech_bundles(SpeechModelId::Qwen3Asr) {
            write_bytes(
                &crate::storage::models_dir(root)
                    .join(&bundle.install_path)
                    .join("w.bin"),
                bytes_each,
            );
        }
        write_speech_marker(root, SpeechModelId::Qwen3Asr).unwrap();
    }

    #[test]
    fn parakeet_manifest_install_stays_ready() {
        // An install from before per-model readiness has only manifest.json.
        let tmp = tempfile::tempdir().unwrap();
        install_parakeet(tmp.path(), 4);
        assert!(speech_is_ready(tmp.path(), SpeechModelId::Parakeet));
        assert!(!speech_is_ready(tmp.path(), SpeechModelId::Qwen3Asr));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn qwen3_ready_requires_its_current_marker() {
        let tmp = tempfile::tempdir().unwrap();
        install_qwen3(tmp.path(), 4);
        assert!(speech_is_ready(tmp.path(), SpeechModelId::Qwen3Asr));
        std::fs::write(
            speech_marker_path(tmp.path(), SpeechModelId::Qwen3Asr).unwrap(),
            "stale",
        )
        .unwrap();
        assert!(!speech_is_ready(tmp.path(), SpeechModelId::Qwen3Asr));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn deleting_parakeet_keeps_the_diarizer_qwen3_needs() {
        let tmp = tempfile::tempdir().unwrap();
        install_parakeet(tmp.path(), 4);
        install_qwen3(tmp.path(), 4);
        delete_speech_model(tmp.path(), SpeechModelId::Parakeet).unwrap();
        let models = crate::storage::models_dir(tmp.path());
        assert!(!models.join("parakeet-tdt-0.6b-v3").exists());
        assert!(models.join("speaker-diarization").exists());
        assert!(!speech_is_ready(tmp.path(), SpeechModelId::Parakeet));
        assert!(speech_is_ready(tmp.path(), SpeechModelId::Qwen3Asr));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn deleting_the_last_speech_model_removes_the_diarizer() {
        let tmp = tempfile::tempdir().unwrap();
        install_qwen3(tmp.path(), 4);
        delete_speech_model(tmp.path(), SpeechModelId::Qwen3Asr).unwrap();
        let models = crate::storage::models_dir(tmp.path());
        assert!(!models.join("qwen3-asr-0.6b-4bit").exists());
        assert!(!models.join("qwen3-forcedaligner-0.6b-4bit").exists());
        assert!(!models.join("speaker-diarization").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn each_speech_model_counts_the_shared_diarizer() {
        let tmp = tempfile::tempdir().unwrap();
        install_parakeet(tmp.path(), 10);
        install_qwen3(tmp.path(), 10);
        assert_eq!(
            speech_model_size(tmp.path(), SpeechModelId::Parakeet),
            Some(20)
        );
        // Qwen3's own readiness marker lives inside its ASR bundle directory
        // (`speech_marker_path`), so its footprint includes the marker's bytes
        // on top of the three 10-byte bundle files.
        let marker_bytes = speech_model_version(SpeechModelId::Qwen3Asr).len() as u64;
        assert_eq!(
            speech_model_size(tmp.path(), SpeechModelId::Qwen3Asr),
            Some(30 + marker_bytes)
        );
    }

    #[test]
    fn speech_size_is_none_when_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(speech_model_size(tmp.path(), SpeechModelId::Parakeet), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn local_models_ready_follows_selection() {
        let tmp = tempfile::tempdir().unwrap();
        install_parakeet(tmp.path(), 4);
        install_notes(tmp.path(), 4); // existing helper: writes the LLM + its marker
        let previous = crate::speech_model::selected();
        crate::speech_model::set_selected(SpeechModelId::Parakeet);
        assert!(local_models_ready(tmp.path()));
        crate::speech_model::set_selected(SpeechModelId::Qwen3Asr);
        assert!(!local_models_ready(tmp.path()));
        assert_eq!(status(tmp.path()).state, "not_downloaded");
        crate::speech_model::set_selected(previous);
    }

    #[test]
    fn qwen3_pins_are_well_formed() {
        for b in macos_qwen3_bundles() {
            assert_eq!(b.prefix.len(), 12);
            assert_eq!(b.manifest_sha256.len(), 64);
            assert!(b.manifest_sha256.bytes().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn model_size_is_none_when_nothing_is_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert_eq!(notes_size(root), None);
        assert_eq!(speech_model_size(root, SpeechModelId::Parakeet), None);
    }

    #[test]
    fn model_size_totals_files_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_bytes(&llm_dir(root).join("model.safetensors"), 1000);
        write_bytes(&llm_dir(root).join("nested").join("extra.bin"), 24);
        assert_eq!(notes_size(root), Some(1024));
    }

    #[test]
    fn model_size_sums_every_speech_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dirs = speech_dirs(root, SpeechModelId::Parakeet);
        assert!(dirs.len() >= 2, "expected asr + diarization bundles");
        install_parakeet(root, 100);
        let total = speech_model_size(root, SpeechModelId::Parakeet).unwrap();
        assert!(total >= (dirs.len() as u64) * 100);
    }

    #[test]
    fn deleting_notes_clears_its_readiness_and_leaves_speech_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        install_notes(root, 10);
        install_parakeet(root, 10);
        assert!(llm_is_ready(root) && speech_is_ready(root, SpeechModelId::Parakeet));

        delete_notes(root).unwrap();

        assert!(!llm_is_ready(root));
        assert_eq!(notes_size(root), None);
        assert!(
            speech_is_ready(root, SpeechModelId::Parakeet),
            "speech must survive a notes delete"
        );
    }

    #[test]
    fn deleting_speech_clears_its_manifest_and_leaves_notes_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        install_notes(root, 10);
        install_parakeet(root, 10);

        delete_speech_model(root, SpeechModelId::Parakeet).unwrap();

        assert!(!speech_is_ready(root, SpeechModelId::Parakeet));
        assert_eq!(speech_model_size(root, SpeechModelId::Parakeet), None);
        assert!(llm_is_ready(root), "notes must survive a speech delete");
    }

    #[test]
    fn deleting_a_model_that_is_not_installed_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        delete_notes(root).unwrap();
        delete_speech_model(root, SpeechModelId::Parakeet).unwrap();
    }

    #[test]
    fn deleting_notes_twice_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        install_notes(root, 10);
        delete_notes(root).unwrap();
        delete_notes(root).unwrap();
        assert!(!llm_is_ready(root));
    }

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
        assert_eq!(s.version.as_deref(), Some(stt_model_version().as_str()));
    }

    #[test]
    fn windows_stt_version_joins_each_pinned_bundle_revision() {
        assert_eq!(
            bundle_version(&windows_stt_bundles()),
            "windows/parakeet-tdt-0.6b-v3@v1+windows/speaker-diarization@v1"
        );
        assert_eq!(
            bundle_version(&windows_llm_bundles()),
            "windows/gemma-3-1b-it-qat-4bit@v3"
        );
    }

    #[test]
    fn llm_ready_requires_complete_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(!llm_is_ready(root));
        assert_eq!(status(root).llm_ready, Some(false));

        // Files present but no marker → a partial download is NOT ready.
        let marker = llm_marker_path(root);
        let dir = marker.parent().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        assert!(!llm_is_ready(root));

        // Marker written only after a full download → ready.
        std::fs::write(&marker, llm_model_version()).unwrap();
        assert!(llm_is_ready(root));
        assert_eq!(status(root).llm_ready, Some(true));

        std::fs::write(&marker, "obsolete").unwrap();
        assert!(!llm_is_ready(root));
    }

    #[test]
    fn llm_marker_path_is_shared_across_platforms() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            llm_marker_path(tmp.path()),
            crate::storage::models_dir(tmp.path())
                .join("llm")
                .join(LLM_MODEL_NAME)
                .join(".complete")
        );
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
        let marker = llm_marker_path(root);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, llm_model_version()).unwrap();
        assert!(local_models_ready(root));
    }

    #[test]
    fn local_models_ready_false_with_llm_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // LLM marker present but no STT manifest → not ready.
        let marker = llm_marker_path(root);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, llm_model_version()).unwrap();
        assert!(!local_models_ready(root));
    }

    #[test]
    fn local_model_status_is_unsupported_on_unknown_platforms() {
        if !(cfg!(target_os = "macos") || cfg!(target_os = "windows")) {
            assert_eq!(local_model_status().unwrap().state, "unsupported");
        }
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
        assert_eq!(
            r2_base!(),
            "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev"
        );
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
        assert!(
            err.contains("model.safetensors"),
            "error should name file: {err}"
        );
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
        assert!(
            st.len() > 700_000_000,
            "safetensors too small: {}",
            st.len()
        );
    }

    #[test]
    fn parse_sha256sums_accepts_text_and_binary_markers() {
        let h = "a".repeat(64);
        let text = format!("{h}  config.json\n{h} *Encoder.mlmodelc/weights/weight.bin\n");
        let entries = parse_sha256sums(&text).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "config.json");
        assert_eq!(entries[1].path, "Encoder.mlmodelc/weights/weight.bin");
        assert_eq!(entries[0].sha256, h);
    }

    #[test]
    fn parse_sha256sums_rejects_malformed_and_empty() {
        assert!(parse_sha256sums("").is_err());
        // hash too short
        assert!(parse_sha256sums("deadbeef  short.json").is_err());
        let h = "a".repeat(64);
        // wrong separator (tab, not two spaces)
        assert!(parse_sha256sums(&format!("{h}\tconfig.json")).is_err());
        // non-hex hash
        assert!(parse_sha256sums(&format!("{}  x.json", "z".repeat(64))).is_err());
    }

    #[test]
    fn parse_sha256sums_rejects_path_traversal() {
        let h = "a".repeat(64);
        assert!(parse_sha256sums(&format!("{h}  ../escape.bin")).is_err());
        assert!(parse_sha256sums(&format!("{h}  /abs/escape.bin")).is_err());
        assert!(parse_sha256sums(&format!("{h}  a/../../b")).is_err());
    }

    #[test]
    fn bundle_file_allowlist_excludes_runtime_entries() {
        let hash = "a".repeat(64);
        let entries =
            parse_sha256sums(&format!("{hash}  model.gguf\n{hash}  llama-server.exe\n")).unwrap();
        let selected = select_manifest_entries(entries, Some(&["model.gguf".into()])).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path, "model.gguf");
    }

    #[test]
    fn sanitize_rel_path_allows_nested_rejects_escapes() {
        assert_eq!(
            sanitize_rel_path("Encoder.mlmodelc/weights/weight.bin").unwrap(),
            PathBuf::from("Encoder.mlmodelc/weights/weight.bin")
        );
        assert!(sanitize_rel_path("").is_err());
        assert!(sanitize_rel_path("../x").is_err());
        assert!(sanitize_rel_path("/x").is_err());
        assert!(sanitize_rel_path("a/../b").is_err());
    }

    #[test]
    fn sha256_hex_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn model_bundle_cdn_pins_are_well_formed() {
        assert!(MODELS_CDN_BASE.starts_with(r2_base!()));
        // Windows bundles live at the bucket root (`windows/...`) of the same R2
        // host as the macOS models and the updater.
        assert_eq!(WINDOWS_MODEL_LOCK.cdn_base, r2_base!());
        let bundles = macos_stt_bundles()
            .into_iter()
            .chain(windows_stt_bundles())
            .chain(windows_llm_bundles());
        for m in bundles {
            assert_eq!(m.manifest_sha256.len(), 64, "{} hash len", m.folder);
            assert!(
                m.manifest_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "{} hash hex",
                m.folder
            );
            assert!(
                m.manifest_sha256.bytes().any(|b| b != b'0'),
                "{} hash must be pinned",
                m.folder
            );
            assert!(!m.prefix.is_empty(), "{} prefix", m.folder);
        }
    }

    // Hits the network (downloads the STT models from R2). Excluded from the
    // default run; invoke with `cargo test stt_r2_download_smoke -- --ignored`.
    #[tokio::test]
    #[ignore = "network: downloads the STT models (~900MB) from the R2 CDN"]
    async fn stt_r2_download_smoke() {
        let tmp = tempfile::tempdir().unwrap();
        let bundles = macos_stt_bundles();
        download_model_bundles(MODELS_CDN_BASE, tmp.path(), &bundles, &|_| {})
            .await
            .unwrap();
        for m in &bundles {
            assert!(
                tmp.path().join(&m.install_path).is_dir(),
                "missing {}",
                m.install_path
            );
        }
        let enc = tmp
            .path()
            .join("parakeet-tdt-0.6b-v3/Encoder.mlmodelc/weights/weight.bin");
        assert!(std::fs::metadata(enc).unwrap().len() > 400_000_000);
    }

    // Hits the network (downloads the Qwen3-ASR models from R2). Excluded from
    // the default run; invoke with `cargo test qwen3_r2_download_smoke -- --ignored`.
    #[tokio::test]
    #[ignore = "network: downloads the Qwen3-ASR models from the R2 CDN"]
    async fn qwen3_r2_download_smoke() {
        let tmp = tempfile::tempdir().unwrap();
        let bundles = macos_qwen3_bundles();
        download_model_bundles(MODELS_CDN_BASE, tmp.path(), &bundles, &|_| {})
            .await
            .unwrap();
        for m in &bundles {
            assert!(
                tmp.path().join(&m.install_path).is_dir(),
                "missing {}",
                m.install_path
            );
        }
    }

    #[tokio::test]
    #[ignore = "network: downloads Windows Local bundles (~1.6GB) from R2"]
    async fn windows_r2_download_smoke() {
        let tmp = tempfile::tempdir().unwrap();
        let speech = windows_stt_bundles();
        let notes = windows_llm_bundles();
        download_model_bundles(&WINDOWS_MODEL_LOCK.cdn_base, tmp.path(), &speech, &|_| {})
            .await
            .unwrap();
        download_model_bundles(&WINDOWS_MODEL_LOCK.cdn_base, tmp.path(), &notes, &|_| {})
            .await
            .unwrap();

        assert!(
            tmp.path()
                .join("windows/parakeet-tdt-0.6b-v3/v1/encoder.int8.onnx")
                .is_file()
        );
        assert!(
            tmp.path()
                .join("windows/speaker-diarization/v1/sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx")
                .is_file()
        );
        assert!(
            tmp.path()
                .join("llm/gemma-3-1b-it-qat-4bit/gemma-3-1b-it-qat-Q4_0.gguf")
                .is_file()
        );
        assert!(
            !tmp.path()
                .join("llm/gemma-3-1b-it-qat-4bit/llama-server.exe")
                .exists()
        );
    }
}
