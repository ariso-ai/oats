use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

#[cfg(windows)]
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::transcript_normalization::{
    normalize_transcript, SidecarTranscriptResult, TranscriptResult,
};

/// Keeps console-subsystem inference binaries behind the GUI application
/// boundary while preserving their piped output for errors and diagnostics.
#[cfg(windows)]
pub(crate) fn configure_background_process(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW.0);
}

#[cfg(not(windows))]
pub(crate) fn configure_background_process(_command: &mut Command) {}

/// Per-recording mutex table. Serializes transcript commits with notes job
/// preparation/commit so detached work cannot race a newer append.
static RECORDING_LOCKS: OnceLock<StdMutex<HashMap<String, Arc<TokioMutex<()>>>>> = OnceLock::new();

pub(crate) fn get_recording_lock(target_id: &str) -> Arc<TokioMutex<()>> {
    let map = RECORDING_LOCKS.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut guard = map.lock().expect("append lock table poisoned");
    guard
        .entry(target_id.to_string())
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone()
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeResult {
    pub backend: String,
    pub id: String,
    pub title: String,
    pub status: crate::storage::RecordingStatus,
}

/// Resolve the platform's `ariso-stt` sidecar from Tauri's externalBin layout.
/// The test-only override never changes production process selection.
pub fn sidecar_path() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(p) = std::env::var_os("ARISO_STT_BIN") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe.parent().ok_or("no parent dir for current_exe")?;
    Ok(dir.join(if cfg!(target_os = "windows") {
        "ariso-stt.exe"
    } else {
        "ariso-stt"
    }))
}

/// Run the sidecar in transcribe mode and parse its JSON stdout.
pub async fn run_transcribe(audio: &Path, models: &Path) -> Result<TranscriptResult, String> {
    let bin = sidecar_path()?;
    let mut command = Command::new(&bin);
    command
        .arg("--audio")
        .arg(audio)
        .arg("--models")
        .arg(models)
        .arg("--format")
        .arg("json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_background_process(&mut command);
    let output = command
        .output()
        .await
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ariso-stt failed: {}", stderr.trim()));
    }
    let raw = serde_json::from_slice::<SidecarTranscriptResult>(&output.stdout).map_err(|e| {
        // Include a bounded, char-safe preview of stdout for diagnosis.
        let preview: String = String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(200)
            .collect();
        format!("parse transcript json: {e} (stdout: {preview})")
    })?;
    Ok(normalize_transcript(raw))
}

use crate::storage::{self, RecordingMeta, RecordingStatus};

/// Pure-ish orchestration over an explicit root, so tests use a tempdir.
///
/// Returns as soon as the transcript is persisted and marked `Done`. Notes
/// generation runs in a detached background task whose `JoinHandle` is
/// returned alongside the result — production callers drop it; tests
/// `.await` it to observe `ari-note.md` / `meta.notes_error`.
pub async fn finalize_core(
    root: &Path,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    finalize_core_with_target(root, audio, title, created_at, duration_seconds, None, false).await
}

/// Like [`finalize_core`], but an explicit `append_to` forces the clip to append
/// to that recording id, bypassing the time-window auto-append decision (used by
/// the "Continue this meeting" flow). `append_recording_core` still re-validates
/// the target is `Done` with audio and falls back to a fresh recording otherwise.
///
/// `force_new` lets a caller skip the 5-minute auto-append window entirely and
/// always start a brand-new recording — used by the "force a new recording"
/// affordance so the user can start fresh even right after a recent meeting
/// ended. Precedence: an explicit `append_to` always wins (an explicit
/// "continue this meeting" request is never overridden by `force_new`); then
/// `force_new`; then the default `most_recent_appendable` auto-append check.
pub async fn finalize_core_with_target(
    root: &Path,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
    append_to: Option<String>,
    force_new: bool,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    if let Some(target_id) = append_to {
        // Defense-in-depth: an explicit append target is externally influenced
        // (round-trips through the recorder window URL). Reject ids that could
        // escape the recordings dir before joining to a path — mirrors
        // validate_recording_id use in retry_transcription_core. Fall back to a
        // fresh recording (audio is never lost) rather than erroring.
        if storage::validate_recording_id(&target_id).is_ok() {
            return append_recording_core(root, &target_id, audio, title, created_at, duration_seconds).await;
        }
        return fresh_recording_core(root, audio, title, created_at, duration_seconds).await;
    }
    if force_new {
        return fresh_recording_core(root, audio, title, created_at, duration_seconds).await;
    }
    match storage::most_recent_appendable(root, &created_at)? {
        Some(target_id) => {
            append_recording_core(root, &target_id, audio, title, created_at, duration_seconds).await
        }
        None => fresh_recording_core(root, audio, title, created_at, duration_seconds).await,
    }
}

/// Create a brand-new recording: persist audio, transcribe, write
/// `segments.json` + `transcript.md`, mark `Done`, spawn notes. On STT failure
/// the recording is marked `Failed` (audio retained).
async fn fresh_recording_core(
    root: &Path,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    let id = storage::sanitize_iso_to_id(&created_at);
    let dir = storage::create_recording_dir(root, &id)?;

    // Persist the audio into the vault first so it is never lost, even if STT
    // fails. The attachment is this recording's only audio home. On an in-place
    // rewrite (a retry re-runs this for the same dir), reuse the existing
    // attachment rather than deriving a fresh unique name — otherwise every
    // retry would orphan the prior file and repoint an existing note's embed at
    // a duplicate.
    let audio_file = match storage::read_meta(&dir).ok().and_then(|m| m.audio_file) {
        Some(existing) => existing, // in-place rewrite (retry): keep the same attachment
        None => format!(
            "{}.mp3",
            crate::vault::unique_basename(
                &crate::vault::ensure_vault()?,
                &crate::vault::note_basename(&created_at, &title, &id),
            )
        ),
    };
    crate::vault::write_audio(&audio_file, &audio)?;

    let mut meta = RecordingMeta {
        id: id.clone(),
        title: title.clone(),
        created_at,
        duration_seconds,
        status: RecordingStatus::Transcribing,
        language: None,
        participants: vec![],
        model_version: None,
        error: None,
        notes_error: None,
        last_clip_end_at: None,
        audio_file: Some(audio_file.clone()),
        notes_written: None,
        notes_cursor: None,
        notes_source_hash: None,
        notes_job_id: None,
        title_is_default: true,
    };
    storage::write_meta(&dir, &meta)?;

    let models = storage::models_dir(&storage::ariso_root()?);
    let audio_path = crate::vault::audio_path(&crate::vault::vault_root()?, &audio_file);
    match run_transcribe(&audio_path, &models).await {
        Ok(result) => {
            meta.language = Some(result.language.clone());
            meta.participants = result.participants.clone();
            meta.model_version = Some(crate::model_manager::stt_model_version());
            storage::write_segments(&dir, &storage::SegmentsFile {
                language: Some(result.language.clone()),
                participants: result.participants.clone(),
                segments: result.segments.clone(),
            })?;
            let md = storage::render_markdown(&meta, &result.segments);
            storage::write_transcript(&dir, &md)?;
            meta.status = RecordingStatus::Done;
            storage::write_meta(&dir, &meta)?;

            // Notes orchestration consumes the persisted transcript/segments
            // through the shared Rust reducer module; STT does not construct
            // prompts or own note-state transitions.
            let notes_handle =
                crate::local_notes::start_notes_job_best_effort(dir.clone(), models).await;

            Ok((
                FinalizeResult {
                    backend: "local".to_string(),
                    id,
                    title,
                    status: RecordingStatus::Done,
                },
                notes_handle,
            ))
        }
        Err(e) => {
            meta.status = RecordingStatus::Failed;
            meta.error = Some(e.clone());
            let _ = storage::write_meta(&dir, &meta);
            Err(e)
        }
    }
}

/// Persist a clip that failed to transcribe as its own `Failed` recording so
/// its audio is never lost. Best-effort: logs on error rather than masking the
/// original transcription failure.
fn save_failed_clip(
    root: &Path,
    audio: &[u8],
    title: &str,
    created_at: &str,
    duration_seconds: u64,
    err: &str,
) {
    let id = storage::sanitize_iso_to_id(created_at);
    match storage::create_recording_dir(root, &id) {
        Ok(dir) => {
            // Persist the audio, preferring the vault. If the vault is
            // unavailable, fall back to the legacy local `recording.mp3`
            // (`audio_file: None`) so the clip stays recoverable and — crucially
            // — the `Failed` record below is still written. Bailing here would
            // drop the whole record, defeating this function's "never lost"
            // guarantee. Audio resolution in commands.rs reads `recording.mp3`
            // when `audio_file` is `None`.
            let audio_file = match crate::vault::ensure_vault() {
                Ok(vault_root) => {
                    let name = format!(
                        "{}.mp3",
                        crate::vault::unique_basename(
                            &vault_root,
                            &crate::vault::note_basename(created_at, title, &id),
                        )
                    );
                    if let Err(e) = crate::vault::write_audio(&name, audio) {
                        eprintln!("save failed clip audio: {e}");
                    }
                    Some(name)
                }
                Err(e) => {
                    eprintln!("save failed clip: ensure vault: {e}; falling back to local dir");
                    if let Err(e) = std::fs::write(dir.join("recording.mp3"), audio) {
                        eprintln!("save failed clip audio (fallback): {e}");
                    }
                    None
                }
            };
            let meta = RecordingMeta {
                id: id.clone(),
                title: title.to_string(),
                created_at: created_at.to_string(),
                duration_seconds,
                status: RecordingStatus::Failed,
                language: None,
                participants: vec![],
                model_version: None,
                error: Some(err.to_string()),
                notes_error: None,
                last_clip_end_at: None,
                audio_file,
                notes_written: None,
                notes_cursor: None,
                notes_source_hash: None,
                notes_job_id: None,
                title_is_default: false,
            };
            let _ = storage::write_meta(&dir, &meta);
        }
        Err(e) => eprintln!("save failed clip dir: {e}"),
    }
}

/// Append a clip to existing recording `target_id`: transcribe just the clip,
/// offset it past the existing content, stitch into `segments.json`, concatenate
/// the audio, update meta, re-render `transcript.md`, and regenerate notes. On
/// STT failure the clip is saved as its own `Failed` recording via
/// [`save_failed_clip`] (no re-transcribe; target untouched). If the target
/// lacks `segments.json` (pre-feature recording), fall back to a fresh
/// recording rather than corrupt it.
///
/// Crash/IO-error safety: once STT succeeds we are committed to merging, so the
/// target is flipped to `Transcribing` and persisted *before* any of its real
/// content files (`recording.mp3`, `segments.json`, `transcript.md`) are
/// touched. A crash or IO error partway through then leaves the target
/// observably incomplete rather than silently `Done` with a stale
/// `duration_seconds` — and `storage::most_recent_appendable` (which requires
/// `Done`) will never pick a mid-append target as the base for a further
/// append, which would otherwise compound the offset bug. The final
/// `write_meta` (status `Done`) is the commit record.
async fn append_recording_core(
    root: &Path,
    target_id: &str,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    let dir = storage::recordings_dir(root).join(target_id);

    // Serialize concurrent appends to the same target recording so two
    // simultaneous finalize calls cannot both see Done status and race on
    // writing audio/segments/meta.
    let append_guard = get_recording_lock(target_id).lock_owned().await;

    // Re-read meta/segments inside the lock. A concurrent append may have
    // completed (and changed status) since finalize_core's
    // most_recent_appendable check. Fall back to a fresh recording if the
    // target's state is no longer suitable for an append.
    let (mut meta, mut existing) = match (storage::read_meta(&dir), storage::read_segments(&dir)) {
        (Ok(m), Ok(Some(s))) if m.status == storage::RecordingStatus::Done && m.audio_file.is_some() => (m, s),
        _ => return fresh_recording_core(root, audio, title, created_at, duration_seconds).await,
    };

    // Capture the pre-append duration as the new clip's time offset before
    // `meta` is mutated any further below.
    let time_offset = meta.duration_seconds as f64;

    let models = storage::models_dir(&storage::ariso_root()?);
    // Transcribe from a temp file so the target's audio is never touched on failure.
    let clip_path = dir.join("append-clip.mp3");
    if let Err(e) = std::fs::write(&clip_path, &audio) {
        save_failed_clip(root, &audio, &title, &created_at, duration_seconds, &format!("write clip: {e}"));
        return Err(format!("write clip: {e}"));
    }
    let result = match run_transcribe(&clip_path, &models).await {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&clip_path);
            // Save this clip as its own Failed recording (no re-transcribe: it
            // would deterministically fail again); leave the target intact.
            save_failed_clip(root, &audio, &title, &created_at, duration_seconds, &e);
            return Err(e);
        }
    };

    // Committed to merging: flip the target to in-progress and persist before
    // touching any of its real content files (see doc comment above).
    meta.status = RecordingStatus::Transcribing;
    storage::write_meta(&dir, &meta)?;

    // Stitch the clip past the existing content.
    let speaker_offset = storage::next_speaker_offset(&existing);
    let mut clip_segments = storage::offset_segments(&result.segments, time_offset, speaker_offset);
    let mut clip_participants = storage::offset_participants(&result.participants, speaker_offset);
    existing.segments.append(&mut clip_segments);
    existing.participants.append(&mut clip_participants);

    // Accumulate meta in memory (not yet persisted) so the transcript
    // re-rendered below reflects the merged duration/participants.
    meta.duration_seconds += duration_seconds;
    meta.participants = existing.participants.clone();
    if meta.language.is_none() {
        meta.language = Some(result.language.clone());
    }
    // Record the true wall-clock end of this clip so subsequent append-window
    // checks use it instead of the audio-only duration sum.
    meta.last_clip_end_at = storage::clip_end_timestamp(&created_at, duration_seconds);

    // Content writes, in crash-safe order: audio, then structured segments,
    // then the rendered transcript. The audio lives only in the vault
    // attachment; read it, concatenate the clip, and write it back.
    let audio_file = meta
        .audio_file
        .clone()
        .ok_or_else(|| "append target has no vault audio (legacy recording)".to_string())?;
    let mut combined = crate::vault::read_audio(&audio_file)?;
    combined.extend_from_slice(&audio);
    crate::vault::write_audio(&audio_file, &combined)?;
    storage::write_segments(&dir, &existing)?;
    let md = storage::render_markdown(&meta, &existing.segments);
    storage::write_transcript(&dir, &md)?;

    // Clear any stale notes_error from a prior failed attempt (mirrors
    // `retry_notes_core`) and commit: status Done, written last.
    meta.notes_error = None;
    meta.status = RecordingStatus::Done;
    storage::write_meta(&dir, &meta)?;
    let _ = std::fs::remove_file(&clip_path);

    // Let note commit/start share the same per-recording boundary without
    // recursively acquiring the append lock held for the merge above.
    drop(append_guard);
    let notes_handle = crate::local_notes::start_notes_job_best_effort(dir.clone(), models).await;
    Ok((
        FinalizeResult {
            backend: "local".to_string(),
            id: target_id.to_string(),
            title: meta.title.clone(),
            status: RecordingStatus::Done,
        },
        notes_handle,
    ))
}

/// Re-run the full pipeline (transcription + notes) for an existing recording,
/// reusing its saved `recording.mp3`. `finalize_core` derives the recording's
/// dir from `created_at`, which equals the existing folder id, so it rewrites
/// in place and resets `status`/`error`/`notes_error`. Returns the result plus
/// the detached notes `JoinHandle` (tests await it; the command drops it).
pub async fn retry_transcription_core(
    root: &Path,
    id: &str,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    storage::validate_recording_id(id)?;
    let dir = storage::recordings_dir(root).join(id);
    let meta = storage::read_meta(&dir)?;
    let derived_id = storage::sanitize_iso_to_id(&meta.created_at);
    if meta.id != id || derived_id != id {
        return Err(format!(
            "recording metadata/id mismatch: requested={id}, meta.id={}, derived={derived_id}",
            meta.id
        ));
    }
    // Prefer the vault attachment; fall back to legacy `~/.ariso` audio so old
    // recordings (pre-vault) can still be retried.
    let audio = match &meta.audio_file {
        Some(audio_file) => crate::vault::read_audio(audio_file)?,
        None => std::fs::read(dir.join("recording.mp3"))
            .map_err(|e| format!("read recording audio: {e}"))?,
    };
    finalize_core(root, audio, meta.title, meta.created_at, meta.duration_seconds).await
}

/// Retry AI notes from the persisted transcript state without re-running STT.
/// The shared notes module selects first-note or reducer mode and leaves any
/// readable prior note in place while the background job runs or fails.
pub async fn retry_notes_core(root: &Path, id: &str) -> Result<JoinHandle<()>, String> {
    storage::validate_recording_id(id)?;
    let dir = storage::recordings_dir(root).join(id);
    if !dir.join("transcript.md").is_file() {
        return Err("no transcript available to generate notes".to_string());
    }
    let models = storage::models_dir(&storage::ariso_root()?);
    crate::local_notes::start_notes_job(dir, models).await
}

/// Retry transcription (and notes) for a failed local recording.
#[tauri::command]
pub async fn retry_local_transcription(id: String) -> Result<FinalizeResult, String> {
    let root = crate::vault::meta_root()?;
    // Drop the notes JoinHandle: notes are best-effort and write their outcome
    // to meta.json directly, matching `local_finalize_recording`.
    retry_transcription_core(&root, &id)
        .await
        .map(|(res, _notes)| res)
}

/// Retry only AI-notes generation for a recording whose transcript exists.
#[tauri::command]
pub async fn retry_local_notes(id: String) -> Result<(), String> {
    let root = crate::vault::meta_root()?;
    // Drop the JoinHandle: the detached task writes its outcome to disk; the
    // frontend observes completion via `local_recording_status` polling.
    retry_notes_core(&root, &id).await.map(|_handle| ())
}

/// Resolve the recording id a new local recording starting at `created_at` will
/// finalize into — the append target (if it will merge into the recent
/// recording) or the new recording's own id. The recorder calls this at start so
/// the library shows the correct row (the current meeting on a resume, a new row
/// otherwise) instead of a phantom new note that vanishes when the append lands.
///
/// `force_new` (default `false` when omitted) mirrors `finalize_core_with_target`'s
/// `force_new`: when `true`, the append-target resolve is skipped entirely and
/// this returns the new recording's own sanitized id, matching what
/// `finalize_core_with_target(..., None, true)` will actually finalize into.
#[tauri::command]
pub async fn local_recording_id_for_start(
    created_at: String,
    force_new: Option<bool>,
) -> Result<String, String> {
    if force_new.unwrap_or(false) {
        return Ok(storage::sanitize_iso_to_id(&created_at));
    }
    let root = crate::vault::meta_root()?;
    storage::resolve_local_recording_id(&root, &created_at)
}

/// Non-binary arguments of [`local_finalize_recording`], carried in the
/// `x-oats-meta` header because the audio itself occupies the request body.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeArgs {
    pub title: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub append_to: Option<String>,
    pub force_new: Option<bool>,
}

/// Takes the mp3 as a raw IPC body rather than a JSON `number[]`: a 50-minute
/// recording is ~48 MB, which costs ~30 s to serialize as numbers versus ~44 ms
/// as a raw body (see `raw_ipc`). That serialization ran on the webview's main
/// thread, freezing the recorder pill while it worked.
#[tauri::command]
pub async fn local_finalize_recording(
    request: tauri::ipc::Request<'_>,
) -> Result<FinalizeResult, String> {
    // Owned: the finalize chain persists the audio and outlives the request.
    let audio = crate::raw_ipc::body_bytes(&request)?.to_vec();
    let FinalizeArgs {
        title,
        created_at,
        duration_seconds,
        append_to,
        force_new,
    } = crate::raw_ipc::meta(&request)?;
    let root = crate::vault::meta_root()?;
    // Drop the notes JoinHandle: notes are best-effort and continue running
    // in the background, writing their outcome to meta.json directly.
    finalize_core_with_target(
        &root,
        audio,
        title,
        created_at,
        duration_seconds,
        append_to,
        force_new.unwrap_or(false),
    )
        .await
        .map(|(res, _notes)| res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(windows)]
    const CONSOLE_PROBE_ENV: &str = "OATS_TEST_CONSOLE_PROBE_CHILD";

    /// Exercises the actual Windows process boundary so a later command
    /// refactor cannot silently bring the console flash back.
    #[cfg(windows)]
    #[tokio::test]
    async fn background_sidecar_child_has_no_console_window() {
        if std::env::var_os(CONSOLE_PROBE_ENV).is_some() {
            let has_console = unsafe {
                !windows::Win32::System::Console::GetConsoleWindow()
                    .0
                    .is_null()
            };
            println!("OATS_CONSOLE_WINDOW={has_console}");
            return;
        }

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("transcribe::tests::background_sidecar_child_has_no_console_window")
            .arg("--nocapture")
            .env(CONSOLE_PROBE_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_background_process(&mut command);

        let output = command.output().await.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "stdout: {stdout}\nstderr: {stderr}");
        assert!(
            stdout.contains("OATS_CONSOLE_WINDOW=false"),
            "child unexpectedly acquired a console window; stdout: {stdout}\nstderr: {stderr}"
        );
    }

    // SAFETY (all set_var/remove_var below): tests run with `--test-threads=1`,
    // so there is no concurrent env mutation while these calls execute.

    #[derive(Clone)]
    struct StubOutcome {
        stdout: String,
        stderr: String,
        success: bool,
    }

    impl StubOutcome {
        fn success(stdout: impl Into<String>) -> Self {
            Self { stdout: stdout.into(), stderr: String::new(), success: true }
        }

        fn failure(stderr: impl Into<String>) -> Self {
            Self { stdout: String::new(), stderr: stderr.into(), success: false }
        }
    }

    struct StubBehavior {
        transcribe: StubOutcome,
        notes: StubOutcome,
        notes_transcript_replacement: Option<String>,
    }

    impl StubBehavior {
        fn transcribe_success(stdout: impl Into<String>) -> Self {
            Self {
                transcribe: StubOutcome::success(stdout),
                notes: StubOutcome::success(
                    "# Meeting Notes\n\n## Summary\n- Clip notes\n\n## Key Points\n\n## Decisions\n\n## Action Items",
                ),
                notes_transcript_replacement: None,
            }
        }

        fn transcribe_failure(stderr: impl Into<String>) -> Self {
            Self {
                transcribe: StubOutcome::failure(stderr),
                notes: StubOutcome::success("# Notes"),
                notes_transcript_replacement: None,
            }
        }

        fn notes_only(notes: StubOutcome) -> Self {
            Self {
                transcribe: StubOutcome::failure("unexpected transcription invocation"),
                notes,
                notes_transcript_replacement: None,
            }
        }

        fn with_notes(mut self, notes: StubOutcome) -> Self {
            self.notes = notes;
            self
        }

        fn replacing_transcript(mut self, replacement: impl Into<String>) -> Self {
            self.notes_transcript_replacement = Some(replacement.into());
            self
        }
    }

    /// Materializes a data-driven sidecar double for either host shell. Payloads
    /// live in files so JSON, Markdown, and shell metacharacters are never parsed
    /// as script source.
    fn write_stub(dir: &Path, behavior: StubBehavior) -> PathBuf {
        std::fs::write(dir.join("transcribe-stdout.txt"), &behavior.transcribe.stdout).unwrap();
        std::fs::write(dir.join("transcribe-stderr.txt"), &behavior.transcribe.stderr).unwrap();
        std::fs::write(dir.join("notes-stdout.txt"), &behavior.notes.stdout).unwrap();
        std::fs::write(dir.join("notes-stderr.txt"), &behavior.notes.stderr).unwrap();
        if let Some(replacement) = behavior.notes_transcript_replacement {
            std::fs::write(dir.join("notes-transcript.txt"), replacement).unwrap();
        }

        let transcribe_exit = i32::from(!behavior.transcribe.success);
        let notes_exit = i32::from(!behavior.notes.success);

        #[cfg(windows)]
        {
            let path = dir.join("stub-stt.cmd");
            let replacement = if dir.join("notes-transcript.txt").is_file() {
                "if exist \"%~dp0notes-transcript.txt\" type \"%~dp0notes-transcript.txt\" > \"%~3\"\r\n"
            } else {
                ""
            };
            let script = format!(
                "@echo off\r\nif \"%~1\"==\"notes\" (\r\n{replacement}type \"%~dp0notes-stdout.txt\"\r\ntype \"%~dp0notes-stderr.txt\" 1>&2\r\nexit /b {notes_exit}\r\n)\r\nif \"%~1\"==\"llm-complete\" (\r\ncopy /Y \"%~3\" \"%~dp0completion-prompt.txt\" >nul\r\ntype \"%~dp0notes-stdout.txt\"\r\ntype \"%~dp0notes-stderr.txt\" 1>&2\r\nexit /b {notes_exit}\r\n)\r\ntype \"%~dp0transcribe-stdout.txt\"\r\ntype \"%~dp0transcribe-stderr.txt\" 1>&2\r\nexit /b {transcribe_exit}\r\n"
            );
            std::fs::write(&path, script).unwrap();
            path
        }

        #[cfg(unix)]
        {
            let path = dir.join("stub-stt.sh");
            let replacement = if dir.join("notes-transcript.txt").is_file() {
                "cat \"$dir/notes-transcript.txt\" > \"$3\"\n"
            } else {
                ""
            };
            let script = format!(
                "#!/bin/sh\ndir=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nif [ \"$1\" = notes ]; then\n{replacement}cat \"$dir/notes-stdout.txt\"\ncat \"$dir/notes-stderr.txt\" >&2\nexit {notes_exit}\nfi\nif [ \"$1\" = llm-complete ]; then\ncp \"$3\" \"$dir/completion-prompt.txt\"\ncat \"$dir/notes-stdout.txt\"\ncat \"$dir/notes-stderr.txt\" >&2\nexit {notes_exit}\nfi\ncat \"$dir/transcribe-stdout.txt\"\ncat \"$dir/transcribe-stderr.txt\" >&2\nexit {transcribe_exit}\n"
            );
            std::fs::write(&path, script).unwrap();
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
            path
        }
    }

    #[tokio::test]
    async fn run_notes_parses_title_and_notes_json() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(
            tmp.path(),
            StubBehavior::notes_only(StubOutcome::success(
                r###"{"title":"Budget Planning","notes":"## Summary body"}"###,
            )),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        let transcript = tmp.path().join("transcript.md");
        std::fs::write(&transcript, b"hi").unwrap();
        let out = crate::local_notes::run_full_notes(&transcript, tmp.path()).await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(out.title.as_deref(), Some("Budget Planning"));
        assert_eq!(out.notes, "## Summary body");
    }

    #[tokio::test]
    async fn run_notes_falls_back_to_raw_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(
            tmp.path(),
            StubBehavior::notes_only(StubOutcome::success("# Notes")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        let transcript = tmp.path().join("transcript.md");
        std::fs::write(&transcript, b"hi").unwrap();
        let out = crate::local_notes::run_full_notes(&transcript, tmp.path()).await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(out.title, None);
        assert_eq!(out.notes, "# Notes");
    }

    #[tokio::test]
    async fn run_notes_blank_title_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(
            tmp.path(),
            StubBehavior::notes_only(StubOutcome::success(
                r###"{"title":"   ","notes":"## Summary"}"###,
            )),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        let transcript = tmp.path().join("transcript.md");
        std::fs::write(&transcript, b"hi").unwrap();
        let out = crate::local_notes::run_full_notes(&transcript, tmp.path()).await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(out.title, None);
        assert_eq!(out.notes, "## Summary");
    }

    #[tokio::test]
    async fn parses_stub_transcript_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_success(json));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let audio = tmp.path().join("a.mp3");
        std::fs::write(&audio, b"x").unwrap();
        let res = run_transcribe(&audio, tmp.path()).await.unwrap();

        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(res.language, "en");
        assert_eq!(res.segments.len(), 1);
        assert_eq!(res.participants[0].label, "Speaker 1");
    }

    #[tokio::test]
    async fn surfaces_stub_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_failure("boom"));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        let audio = tmp.path().join("a.mp3");
        std::fs::write(&audio, b"x").unwrap();
        let err = run_transcribe(&audio, tmp.path()).await.unwrap_err();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert!(err.contains("boom"), "got: {err}");
    }

    use crate::storage::{read_meta, RecordingStatus};

    #[tokio::test]
    async fn finalize_writes_transcript_and_marks_done() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_success(json));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        // Vault resolves via ARISO_ROOT; point it at the same tempdir as `root`
        // so audio writes stay inside this test's sandbox.
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "My Title".into(), "2026-06-02T14:30:05.000Z".into(), 12,
        ).await.unwrap();
        // Drain the notes task before clearing ARISO_STT_BIN, otherwise the
        // detached task would race the env-var teardown.
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(res.status, RecordingStatus::Done);
        assert_eq!(res.id, "2026-06-02T14-30-05Z");
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let meta = read_meta(&dir).unwrap();
        assert_eq!(
            crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(),
            b"audio",
            "audio must live in the vault attachment"
        );
        assert!(!dir.join("recording.mp3").exists(), "no ~/.ariso audio copy");
        assert!(dir.join("transcript.md").exists());
        assert_eq!(meta.status, RecordingStatus::Done);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn finalize_writes_segments_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":1.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_success(json));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05.000Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        unsafe { std::env::remove_var("ARISO_ROOT"); }

        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let seg = crate::storage::read_segments(&dir).unwrap().expect("segments.json written");
        assert_eq!(seg.segments.len(), 1);
        assert_eq!(seg.participants[0].label, "Speaker 1");
    }

    #[tokio::test]
    async fn finalize_marks_failed_but_keeps_audio_on_stt_error() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_failure("boom"));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let err = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05Z".into(), 5,
        ).await.unwrap_err();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert!(err.contains("boom"), "got: {err}");
        let dir = crate::storage::recordings_dir(tmp.path()).join("2026-06-02T14-30-05Z");
        // STT fails *after* the vault write, so the attachment is retained even
        // though no ~/.ariso copy exists.
        let meta = read_meta(&dir).unwrap();
        assert_eq!(
            crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(),
            b"audio",
            "audio must be retained in the vault"
        );
        assert!(!dir.join("recording.mp3").exists());
        assert_eq!(meta.status, RecordingStatus::Failed);
        assert!(meta.error.is_some());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn finalize_writes_vault_note_when_notes_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::success("# Notes\n- did a thing")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(res.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        // Note lands in the vault (resolvable by oats_id), not ~/.ariso.
        let notes = crate::vault::read_note(&res.id).unwrap();
        assert!(notes.as_deref().unwrap_or("").contains("# Notes"), "got: {notes:?}");
        assert!(!dir.join("ari-note.md").exists());
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert!(meta.notes_error.is_none());
        assert!(meta.notes_written.is_some());
        assert_eq!(meta.notes_cursor, Some(1));
        assert!(meta.notes_source_hash.is_some());
        assert!(meta.notes_job_id.is_none());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn finalize_writes_vault_note_not_ari_note_md() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::success("# Vault note")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        // Note is in the vault, resolvable by oats_id; NOT in ~/.ariso.
        let id = storage::sanitize_iso_to_id("2026-06-02T10:00:00Z");
        let note = crate::vault::read_note(&id).unwrap().unwrap();
        assert!(note.starts_with("# Meeting Notes"));
        assert!(note.contains("## Summary\n# Vault note"));
        assert!(note.contains("## Action Items"));
        let dir = storage::recordings_dir(tmp.path()).join(&id);
        assert!(!dir.join("ari-note.md").exists());
        // notes_written stamped.
        assert!(storage::read_meta(&dir).unwrap().notes_written.is_some());
        assert_eq!(res.id, id);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn finalize_stays_done_when_notes_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::failure("notes boom")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(res.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        assert!(dir.join("transcript.md").exists());
        assert!(!dir.join("ari-note.md").exists());
        assert!(crate::vault::read_note(&res.id).unwrap().is_none(), "no vault note on notes failure");
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Done);
        assert!(meta.notes_error.is_some());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn retry_transcription_reruns_failed_recording_to_done() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Seed a previously-failed recording: audio on disk, meta=failed.
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("recording.mp3"), b"audio").unwrap();
        let meta = RecordingMeta {
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: RecordingStatus::Failed,
            language: None,
            participants: vec![],
            model_version: None,
            error: Some("old failure".into()),
            notes_error: None,
            last_clip_end_at: None,
            audio_file: None,
            notes_written: None,
            notes_cursor: None,
            notes_source_hash: None,
            notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let json = r#"{"language":"en","durationSeconds":5.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_success(json));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = retry_transcription_core(root, id).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        unsafe { std::env::remove_var("ARISO_ROOT"); }

        assert_eq!(res.status, RecordingStatus::Done);
        assert!(dir.join("transcript.md").exists());
        let meta = read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Done);
        assert!(meta.error.is_none(), "prior transcription error must be cleared");
    }

    #[tokio::test]
    async fn retry_transcription_errors_when_audio_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        // meta but no recording.mp3
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Failed, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let err = retry_transcription_core(root, id).await.unwrap_err();
        assert!(err.contains("recording audio"), "got: {err}");
    }

    #[tokio::test]
    async fn retry_notes_regenerates_from_existing_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Isolate the vault lookup `retry_notes_core` now performs so it can't
        // touch the real ~/.ariso.
        unsafe { std::env::set_var("ARISO_ROOT", root); }
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"# Transcript\nhi").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None,
            notes_error: Some("prior notes failure".into()),
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let stub = write_stub(
            tmp.path(),
            StubBehavior::notes_only(StubOutcome::success("# Notes\n- point")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let handle = retry_notes_core(root, id).await.unwrap();
        handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        // Legacy recording (no audio_file): the note still lands in ari-note.md.
        let notes = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert!(notes.contains("# Notes"), "got: {notes}");
        assert!(read_meta(&dir).unwrap().notes_error.is_none(), "notes_error must be cleared");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn reducer_success_uses_only_delta_and_advances_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        unsafe { std::env::set_var("ARISO_ROOT", root); }
        let asr = r#"{"language":"en","durationSeconds":1.0,"segments":[{"speaker":"model-speaker-0","text":"old discussion","start":0.0,"end":1.0}]}"#;
        let initial_note = "# Meeting Notes\n\n## Summary\nOld recap\n\n## Key Points\n- Existing point\n\n## Decisions\n- Keep policy in Rust\n\n## Action Items\n- Alex: test Windows";
        let first_stub = write_stub(
            root,
            StubBehavior::transcribe_success(asr)
                .with_notes(StubOutcome::success(initial_note)),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &first_stub); }

        let (recording, first_job) = finalize_core(
            root, b"audio".to_vec(), "Reducer".into(),
            "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        first_job.await.unwrap();
        let dir = storage::recordings_dir(root).join(&recording.id);
        let first_meta = storage::read_meta(&dir).unwrap();
        assert_eq!(first_meta.notes_cursor, Some(1));

        let mut segments = storage::read_segments(&dir).unwrap().unwrap();
        segments.segments.push(storage::Segment {
            speaker: 0,
            text: "the Windows adapter uses the shared completion contract".into(),
            start: 2.0,
            end: 3.0,
        });
        storage::write_segments(&dir, &segments).unwrap();
        storage::write_transcript(
            &dir,
            &storage::render_markdown(&first_meta, &segments.segments),
        ).unwrap();

        let reduced = "# Meeting Notes\n\n## Summary\nUpdated recap\n\n## Key Points\n- Shared completion contract\n\n## Decisions\n\n## Action Items";
        let reducer_stub = write_stub(
            root,
            StubBehavior::notes_only(StubOutcome::success(reduced)),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &reducer_stub); }
        let reducer_job = retry_notes_core(root, &recording.id).await.unwrap();
        reducer_job.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        let note = crate::vault::read_note(&recording.id).unwrap().unwrap();
        assert!(note.contains("## Summary\nUpdated recap"));
        assert!(note.contains("## Decisions\n- Keep policy in Rust"));
        assert!(note.contains("## Action Items\n- Alex: test Windows"));
        let meta = storage::read_meta(&dir).unwrap();
        assert_eq!(meta.notes_cursor, Some(2));
        assert_ne!(meta.notes_source_hash, first_meta.notes_source_hash);
        assert!(meta.notes_written.is_some());
        assert!(meta.notes_error.is_none());
        assert!(meta.notes_job_id.is_none());

        let prompt = std::fs::read_to_string(root.join("completion-prompt.txt")).unwrap();
        let delta = prompt
            .split("<new-transcript-delta>\n").nth(1).unwrap()
            .split("\n</new-transcript-delta>").next().unwrap();
        assert!(delta.contains("shared completion contract"));
        assert!(!delta.contains("old discussion"));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn reducer_blank_output_preserves_note_cursor_and_timestamp() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        unsafe { std::env::set_var("ARISO_ROOT", root); }
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        let participants = vec![storage::Participant { id: 0, label: "Speaker 1".into() }];
        let segments = storage::SegmentsFile {
            language: Some("en".into()),
            participants: participants.clone(),
            segments: vec![
                storage::Segment { speaker: 0, text: "old fact".into(), start: 0.0, end: 1.0 },
                storage::Segment { speaker: 0, text: "new fact".into(), start: 2.0, end: 3.0 },
            ],
        };
        let old_note = "# Meeting Notes\n\n## Summary\nLast good note\n\n## Key Points\n\n## Decisions\n- Keep it\n\n## Action Items";
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 3, status: RecordingStatus::Done, language: Some("en".into()),
            participants, model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None,
            notes_written: Some("2026-06-02T14:31:00Z".into()), notes_cursor: Some(1),
            notes_source_hash: Some("last-good-hash".into()), notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();
        storage::write_segments(&dir, &segments).unwrap();
        storage::write_transcript(&dir, &storage::render_markdown(&meta, &segments.segments)).unwrap();
        storage::write_notes(&dir, old_note).unwrap();
        let stub = write_stub(root, StubBehavior::notes_only(StubOutcome::success("")));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let job = retry_notes_core(root, id).await.unwrap();
        job.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(std::fs::read_to_string(dir.join("ari-note.md")).unwrap(), old_note);
        let failed = storage::read_meta(&dir).unwrap();
        assert_eq!(failed.notes_cursor, Some(1));
        assert_eq!(failed.notes_source_hash.as_deref(), Some("last-good-hash"));
        assert_eq!(failed.notes_written.as_deref(), Some("2026-06-02T14:31:00Z"));
        assert!(failed.notes_error.as_deref().unwrap().contains("empty output"));
        assert!(failed.notes_job_id.is_none());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn retry_notes_failure_preserves_legacy_note() {
        // A retry replaces a note only after a successful job. The prior note
        // remains the readable fallback while the sidecar runs or fails.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Isolate the vault lookup `retry_notes_core` now performs so it can't
        // touch the real ~/.ariso.
        unsafe { std::env::set_var("ARISO_ROOT", root); }
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"# Transcript\nhi").unwrap();
        std::fs::write(dir.join("ari-note.md"), b"# Old note").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let stub = write_stub(
            tmp.path(),
            StubBehavior::notes_only(StubOutcome::failure("boom")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let handle = retry_notes_core(root, id).await.unwrap();
        handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(
            std::fs::read_to_string(dir.join("ari-note.md")).unwrap(),
            "# Old note"
        );
        assert!(read_meta(&dir).unwrap().notes_error.is_some());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn retry_notes_without_delta_keeps_vault_note() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = tmp.path().to_path_buf();

        let json = r#"{"language":"en","durationSeconds":1.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            &root,
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::success("# first")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, h) = finalize_core(
            &root, b"a".to_vec(), "N".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        h.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        let first_note = crate::vault::read_note(&res.id).unwrap().unwrap();
        assert!(first_note.contains("## Summary\n# first"));
        let dir = storage::recordings_dir(&root).join(&res.id);
        let first_meta = storage::read_meta(&dir).unwrap();
        let first_written = first_meta.notes_written.clone();
        assert_eq!(first_meta.notes_cursor, Some(1));

        // A ready note whose cursor covers every segment is a no-op, even if a
        // different sidecar response would be available.
        let stub2 = write_stub(
            &root,
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::success("# second")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub2); }

        let h2 = retry_notes_core(&root, &res.id).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(crate::vault::read_note(&res.id).unwrap().unwrap(), first_note);
        let retried_meta = storage::read_meta(&dir).unwrap();
        assert_eq!(retried_meta.notes_written, first_written);
        assert_eq!(retried_meta.notes_cursor, Some(1));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn reducer_failure_preserves_vault_note_and_cursor() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = tmp.path().to_path_buf();

        let json = r#"{"language":"en","durationSeconds":1.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            &root,
            StubBehavior::transcribe_success(json)
                .with_notes(StubOutcome::success("# first")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, h) = finalize_core(
            &root, b"a".to_vec(), "N".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        h.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        let original_note = crate::vault::read_note(&res.id).unwrap().unwrap();
        let dir = storage::recordings_dir(&root).join(&res.id);
        let original_meta = storage::read_meta(&dir).unwrap();
        assert_eq!(original_meta.notes_cursor, Some(1));

        let mut segments = storage::read_segments(&dir).unwrap().unwrap();
        segments.segments.push(storage::Segment {
            speaker: 0,
            text: "new information".into(),
            start: 2.0,
            end: 3.0,
        });
        storage::write_segments(&dir, &segments).unwrap();
        storage::write_transcript(
            &dir,
            &storage::render_markdown(&original_meta, &segments.segments),
        )
        .unwrap();

        // Regeneration now fails outright.
        let fail_stub = write_stub(
            &root,
            StubBehavior::notes_only(StubOutcome::failure("boom")),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &fail_stub); }

        let h2 = retry_notes_core(&root, &res.id).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(crate::vault::read_note(&res.id).unwrap().unwrap(), original_note);
        let failed_meta = storage::read_meta(&dir).unwrap();
        assert_eq!(failed_meta.notes_cursor, Some(1));
        assert_eq!(failed_meta.notes_written, original_meta.notes_written);
        assert!(failed_meta.notes_error.is_some());
        assert!(failed_meta.notes_job_id.is_none());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    // Each append test receives the same deterministic clip and notes payload.
    fn clip_stub(dir: &Path) -> PathBuf {
        let json = r#"{"language":"en","durationSeconds":3.0,"segments":[{"speaker":"model-speaker-0","text":"clip text","start":0.0,"end":3.0}]}"#;
        write_stub(dir, StubBehavior::transcribe_success(json))
    }

    #[tokio::test]
    async fn second_recording_within_window_appends_to_first() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // First recording: 30s, ends 10:00:30.
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Second recording starts 10:01:00 (30s after end) → appends.
        let (r2, h2) = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        // Same recording id, no second directory.
        assert_eq!(r2.id, r1.id);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        let seg = crate::storage::read_segments(&dir).unwrap().unwrap();
        assert_eq!(seg.segments.len(), 2, "both clips stitched");
        // Second clip offset past the first: start = prior duration (30s).
        assert_eq!(seg.segments[1].start, 30.0);
        // Speaker ids kept distinct.
        assert_eq!(seg.segments[0].speaker, 0);
        assert_eq!(seg.segments[1].speaker, 1);
        // Meta duration summed; audio concatenated.
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.duration_seconds, 45);
        assert_eq!(
            crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(),
            b"aaabbb",
            "concatenated audio lives in the vault attachment"
        );
        assert!(!dir.join("recording.mp3").exists());
        // transcript.md re-rendered with both clips.
        let md = std::fs::read_to_string(dir.join("transcript.md")).unwrap();
        assert_eq!(md.matches("clip text").count(), 2);
        // Only one recording directory exists.
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 1);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn second_recording_outside_window_is_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();
        // 20 minutes later → separate recording.
        let (r2, h2) = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:20:00.000Z".into(), 15,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        unsafe { std::env::remove_var("ARISO_ROOT"); }

        assert_ne!(r2.id, r1.id);
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn forced_append_outside_window_appends_to_target() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // First recording: 30s, ends 10:00:30.
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Second recording 20 minutes later — WELL outside the 5-min auto window —
        // but with an explicit append target, it must still append to r1.
        let (r2, h2) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:20:00.000Z".into(), 15,
            Some(r1.id.clone()), false,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(r2.id, r1.id, "forced append must merge into the target");
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        let seg = crate::storage::read_segments(&dir).unwrap().unwrap();
        assert_eq!(seg.segments.len(), 2, "both clips stitched");
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.duration_seconds, 45);
        assert_eq!(
            crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(),
            b"aaabbb",
        );
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 1, "no second recording directory");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn forced_append_to_missing_target_falls_back_to_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // No such target on disk → append_recording_core re-validates and falls
        // back to a fresh recording keyed by the clip's own created_at.
        let (r, h) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T".into(), "2026-06-02T10:01:00.000Z".into(), 15,
            Some("2026-01-01T00-00-00Z".into()), false,
        ).await.unwrap();
        h.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(r.id, "2026-06-02T10-01-00Z");
        assert_eq!(r.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r.id);
        assert_eq!(crate::vault::read_audio(
            crate::storage::read_meta(&dir).unwrap().audio_file.as_ref().unwrap()).unwrap(), b"bbb");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn forced_append_rejects_invalid_target_id_and_falls_back_to_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // A traversal-shaped target id must never be joined to a path; it
        // degrades to a fresh recording keyed by the clip's own created_at,
        // same as an explicit target that fails validate_recording_id.
        let (r, h) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T".into(), "2026-06-02T10:01:00.000Z".into(), 15,
            Some("../evil".into()), false,
        ).await.unwrap();
        h.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(r.id, "2026-06-02T10-01-00Z");
        assert_eq!(r.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r.id);
        assert_eq!(crate::vault::read_audio(
            crate::storage::read_meta(&dir).unwrap().audio_file.as_ref().unwrap()).unwrap(), b"bbb");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn force_new_starts_fresh_recording_even_within_append_window() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // First recording: 30s, ends 10:00:30.
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Second clip starts 10:01:00 — well WITHIN the 5-min auto-append
        // window — but force_new=true must skip most_recent_appendable and
        // start its own fresh recording instead of merging into r1.
        let (r2, h2) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
            None, true,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_ne!(r2.id, r1.id, "force_new must bypass the auto-append target");
        assert_eq!(r2.id, "2026-06-02T10-01-00Z");
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 2, "two distinct recording directories, no merge");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn force_new_false_still_auto_appends_within_window() {
        // Regression: force_new=false with no explicit append_to must preserve
        // the existing 5-minute auto-append behavior.
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (r1, h1) = finalize_core_with_target(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
            None, false,
        ).await.unwrap();
        h1.await.unwrap();

        let (r2, h2) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
            None, false,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(r2.id, r1.id, "force_new=false must still auto-append within the window");
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 1);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn explicit_append_to_wins_over_force_new() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // force_new=true AND an explicit append_to target: the explicit
        // continue-this-meeting request must win over force_new.
        let (r2, h2) = finalize_core_with_target(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:20:00.000Z".into(), 15,
            Some(r1.id.clone()), true,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(r2.id, r1.id, "explicit append_to must win over force_new");
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        let seg = crate::storage::read_segments(&dir).unwrap().unwrap();
        assert_eq!(seg.segments.len(), 2, "both clips stitched into the explicit target");
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 1, "no second recording directory");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn local_recording_id_for_start_force_new_returns_own_id_not_append_target() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();

        // Seed a recent Done recording that would normally be the append
        // target: created 10:00:00, ran 60s, so it's "appendable" through
        // 10:06:00 (60s + 300s window).
        let id = "2026-06-02T10-00-00Z";
        let dir = storage::create_recording_dir(&root, id).unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T10:00:00.000Z".into(),
            duration_seconds: 60, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Without force_new: resolves to the append target (regression check,
        // mirrors storage::resolve_local_recording_id_targets_recent_else_new).
        let resolved = local_recording_id_for_start("2026-06-02T10:02:00.000Z".into(), None)
            .await
            .unwrap();
        assert_eq!(resolved, id);

        // With force_new=true: bypasses most_recent_appendable entirely and
        // returns the new recording's own sanitized id.
        let forced = local_recording_id_for_start("2026-06-02T10:02:00.000Z".into(), Some(true))
            .await
            .unwrap();
        assert_eq!(forced, storage::sanitize_iso_to_id("2026-06-02T10:02:00.000Z"));
        assert_ne!(forced, id);

        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn append_stt_failure_leaves_target_untouched_and_saves_failed_clip() {
        let tmp = tempfile::tempdir().unwrap();
        // First: success stub.
        let ok_stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &ok_stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Second: failing stub → append must fall back to a separate Failed recording.
        let fail_stub = write_stub(tmp.path(), StubBehavior::transcribe_failure("boom"));
        unsafe { std::env::set_var("ARISO_STT_BIN", &fail_stub); }
        let err = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
        ).await.unwrap_err();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert!(err.contains("boom"), "got: {err}");

        // Target untouched: still one segment, 30s, audio "aaa" in the vault.
        let dir1 = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        assert_eq!(crate::storage::read_segments(&dir1).unwrap().unwrap().segments.len(), 1);
        let meta1 = crate::storage::read_meta(&dir1).unwrap();
        assert_eq!(meta1.duration_seconds, 30);
        assert_eq!(crate::vault::read_audio(meta1.audio_file.as_ref().unwrap()).unwrap(), b"aaa");

        // The failed clip is its own recording, audio retained in the vault, status Failed.
        let dir2 = crate::storage::recordings_dir(tmp.path()).join("2026-06-02T10-01-00Z");
        let meta2 = crate::storage::read_meta(&dir2).unwrap();
        assert_eq!(crate::vault::read_audio(meta2.audio_file.as_ref().unwrap()).unwrap(), b"bbb");
        assert_eq!(meta2.status, RecordingStatus::Failed);
        assert!(!dir1.join("append-clip.mp3").exists(), "temp clip cleaned up");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn save_failed_clip_falls_back_to_local_dir_when_vault_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        // Block the vault: make `<ariso_root>/vault` a file so ensure_vault's
        // create_dir_all(vault/Attachments) fails, while the recording dir (a
        // sibling under recordings/) is still created fine.
        std::fs::write(crate::vault::vault_root().unwrap(), b"x").unwrap();

        save_failed_clip(tmp.path(), b"clipbytes", "T", "2026-06-02T10:00:00.000Z", 15, "boom");

        // The Failed record is still written despite the vault being unavailable
        // — bailing early here would lose the clip entirely.
        let dir = crate::storage::recordings_dir(tmp.path()).join("2026-06-02T10-00-00Z");
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Failed);
        assert_eq!(meta.error.as_deref(), Some("boom"));
        // Audio falls back to the legacy local path (audio_file: None → read from
        // `<dir>/recording.mp3`).
        assert!(meta.audio_file.is_none(), "vault-less clip must use legacy audio path");
        assert_eq!(std::fs::read(dir.join("recording.mp3")).unwrap(), b"clipbytes");

        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn append_falls_back_to_fresh_when_target_segments_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // First recording: 30s, ends 10:00:30.
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Corrupt the target's segments.json so `read_segments` returns Err,
        // simulating on-disk corruption rather than a missing pre-feature sidecar.
        let dir1 = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        std::fs::write(dir1.join("segments.json"), b"not json").unwrap();

        // Second clip starts 10:01:00 (within the append window) — should fall
        // back to a fresh recording rather than lose the clip's audio.
        let (r2, h2) = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        // Did NOT append: a second recording directory now exists, keyed by the
        // clip's own created_at.
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.id, "2026-06-02T10-01-00Z");
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 2);

        // The clip's audio is preserved as its own fresh recording in the vault.
        let dir2 = crate::storage::recordings_dir(tmp.path()).join(&r2.id);
        let meta2 = crate::storage::read_meta(&dir2).unwrap();
        assert_eq!(crate::vault::read_audio(meta2.audio_file.as_ref().unwrap()).unwrap(), b"bbb");
        assert_eq!(meta2.status, RecordingStatus::Done);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn append_clears_prior_notes_error() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        // First recording: 30s, ends 10:00:30.
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Seed a prior notes failure on the target, as if an earlier notes
        // generation attempt had failed.
        let dir = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        let mut meta = storage::read_meta(&dir).unwrap();
        meta.notes_error = Some("prior notes failure".into());
        storage::write_meta(&dir, &meta).unwrap();

        // Second recording within window → appends and regenerates notes.
        let (r2, h2) = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        unsafe { std::env::remove_var("ARISO_ROOT"); }

        assert_eq!(r2.id, r1.id);
        let meta = storage::read_meta(&dir).unwrap();
        assert!(meta.notes_error.is_none(), "append must clear a stale notes_error");
    }

    #[tokio::test]
    async fn retry_notes_errors_without_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let err = retry_notes_core(root, id).await.unwrap_err();
        assert!(err.contains("transcript"), "got: {err}");
    }

    #[tokio::test]
    async fn notes_job_rejects_a_transcript_change_midflight() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        unsafe { std::env::set_var("ARISO_ROOT", root); }
        let id = "2026-06-02T10-00-00Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"original").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T10:00:00Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
            notes_cursor: None, notes_source_hash: None, notes_job_id: None,
            title_is_default: false,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Simulate an append landing while notes are generating. The job must
        // reject output derived from the superseded transcript.
        let stub = write_stub(
            root,
            StubBehavior::notes_only(StubOutcome::success("# Notes"))
                .replacing_transcript("changed"),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let handle = crate::local_notes::start_notes_job(dir.clone(), storage::models_dir(root))
            .await
            .unwrap();
        handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert!(!dir.join("ari-note.md").exists(), "superseded notes must be discarded");
        let meta = storage::read_meta(&dir).unwrap();
        assert!(meta.notes_error.as_deref().unwrap().contains("source changed"));
        assert!(meta.notes_job_id.is_none());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn fresh_recording_stores_audio_in_vault_only() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = tmp.path().to_path_buf();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, notes_handle) = finalize_core(
            &root, b"audiobytes".to_vec(), "Team Standup".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        let dir = storage::recordings_dir(&root).join(&res.id);
        let meta = storage::read_meta(&dir).unwrap();
        let audio_file = meta.audio_file.clone().expect("audio_file set");
        assert_eq!(audio_file, "2026-06-02 Team Standup.mp3");
        assert_eq!(crate::vault::read_audio(&audio_file).unwrap(), b"audiobytes");
        assert!(!dir.join("recording.mp3").exists(), "no ~/.ariso audio copy");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn append_concatenates_audio_in_vault() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = tmp.path().to_path_buf();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        // First clip → fresh recording.
        let (r1, h1) = finalize_core(
            &root, b"AAAA".to_vec(), "Chat".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        h1.await.unwrap();
        // Second clip within the append window → merges into r1.
        let (r2, h2) = finalize_core(
            &root, b"BBBB".to_vec(), "Chat".into(), "2026-06-02T10:00:30Z".into(), 1,
        ).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(r1.id, r2.id, "second clip appended to the first");

        let dir = storage::recordings_dir(&root).join(&r2.id);
        let meta = storage::read_meta(&dir).unwrap();
        assert_eq!(crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(), b"AAAABBBB");
        assert!(!dir.join("recording.mp3").exists());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[tokio::test]
    async fn retry_reads_audio_from_vault() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = tmp.path().to_path_buf();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        let (res, h) = finalize_core(
            &root, b"origaudio".to_vec(), "Sync".into(), "2026-06-02T10:00:00Z".into(), 1,
        ).await.unwrap();
        h.await.unwrap();
        // Retry should re-run using the vault audio without error.
        let (res2, h2) = retry_transcription_core(&root, &res.id).await.unwrap();
        h2.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert_eq!(res2.id, res.id);
        let meta = storage::read_meta(&storage::recordings_dir(&root).join(&res.id)).unwrap();
        assert_eq!(crate::vault::read_audio(meta.audio_file.as_ref().unwrap()).unwrap(), b"origaudio");
        // Retry must REUSE the original attachment, not orphan a duplicate.
        assert_eq!(meta.audio_file.as_deref(), Some("2026-06-02 Sync.mp3"));
        let attachments = crate::vault::attachments_dir(&crate::vault::vault_root().unwrap());
        let count = std::fs::read_dir(&attachments).unwrap().count();
        assert_eq!(count, 1, "retry must not orphan a duplicate attachment");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn generated_title_skipped_when_not_default() {
        let mut meta = RecordingMeta {
            id: "2026-06-02T14-30-05Z".into(),
            title: "User Title".into(),
            created_at: "2026-06-02T14:30:05.000Z".into(),
            duration_seconds: 12,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
            last_clip_end_at: None,
            audio_file: Some("2026-06-02 User Title.mp3".into()),
            notes_written: None,
            notes_cursor: None,
            notes_source_hash: None,
            notes_job_id: None,
            title_is_default: false,
        };
        crate::local_notes::maybe_apply_generated_title(
            &mut meta,
            "2026-06-02 User Title.mp3",
            Some("Generated".into()),
        );
        assert_eq!(meta.title, "User Title");
        assert!(!meta.title_is_default);
    }

    #[test]
    fn generated_title_skipped_when_blank() {
        let mut meta = RecordingMeta {
            id: "2026-06-02T14-30-05Z".into(),
            title: "Mon Jul 6 @ 959AM".into(),
            created_at: "2026-06-02T14:30:05.000Z".into(),
            duration_seconds: 12,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
            last_clip_end_at: None,
            audio_file: Some("2026-06-02 default.mp3".into()),
            notes_written: None,
            notes_cursor: None,
            notes_source_hash: None,
            notes_job_id: None,
            title_is_default: true,
        };
        crate::local_notes::maybe_apply_generated_title(
            &mut meta,
            "2026-06-02 default.mp3",
            None,
        );
        assert_eq!(meta.title, "Mon Jul 6 @ 959AM");
        assert!(meta.title_is_default);
    }

    #[test]
    fn generated_title_skipped_when_whitespace() {
        let mut meta = RecordingMeta {
            id: "2026-06-02T14-30-05Z".into(),
            title: "Mon Jul 6 @ 959AM".into(),
            created_at: "2026-06-02T14:30:05.000Z".into(),
            duration_seconds: 12,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
            last_clip_end_at: None,
            audio_file: Some("2026-06-02 default.mp3".into()),
            notes_written: None,
            notes_cursor: None,
            notes_source_hash: None,
            notes_job_id: None,
            title_is_default: true,
        };
        crate::local_notes::maybe_apply_generated_title(
            &mut meta,
            "2026-06-02 default.mp3",
            Some("   ".to_string()),
        );
        assert_eq!(meta.title, "Mon Jul 6 @ 959AM");
        assert!(meta.title_is_default);
    }

    #[tokio::test]
    async fn notes_regenerate_default_title_and_rename_vault_note() {
        let tmp = tempfile::tempdir().unwrap();
        let asr = r#"{"language":"en","durationSeconds":12.0,"segments":[{"speaker":"model-speaker-0","text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(asr).with_notes(StubOutcome::success(
                r###"{"title":"Budget Planning","notes":"## Summary body"}"###,
            )),
        );
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "Mon Jul 6 @ 959AM".into(), "2026-06-02T14:30:05.000Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let meta = read_meta(&dir).unwrap();
        assert_eq!(meta.title, "Budget Planning");
        assert!(!meta.title_is_default);
        assert_eq!(meta.audio_file.as_deref(), Some("2026-06-02 Budget Planning.mp3"));
        // The vault note file was renamed to the generated-title basename.
        let vault = crate::vault::vault_root().unwrap();
        assert!(
            vault.join("2026-06-02 Budget Planning.md").exists(),
            "expected renamed vault note; vault had: {:?}",
            std::fs::read_dir(&vault).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>()
        );
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }
}
