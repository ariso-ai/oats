use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

/// Upper bound on how long the notes sidecar may run before we kill it.
/// Notes generation is best-effort and runs detached from `finalize_core`,
/// so this only bounds the background task's lifetime.
const NOTES_TIMEOUT: Duration = Duration::from_secs(300);

/// Per-recording mutex table. Serializes concurrent `append_recording_core`
/// calls to the same target so two simultaneous finalizations cannot both
/// see a `Done` target and race on writing audio/segments/meta.
static APPEND_LOCKS: OnceLock<StdMutex<HashMap<String, Arc<TokioMutex<()>>>>> = OnceLock::new();

fn get_append_lock(target_id: &str) -> Arc<TokioMutex<()>> {
    let map = APPEND_LOCKS.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut guard = map.lock().expect("append lock table poisoned");
    guard
        .entry(target_id.to_string())
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone()
}


#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptResult {
    pub language: String,
    pub participants: Vec<crate::storage::Participant>,
    pub segments: Vec<crate::storage::Segment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeResult {
    pub backend: String,
    pub id: String,
    pub title: String,
    pub status: crate::storage::RecordingStatus,
}

/// Resolve the `ariso-stt` sidecar. `ARISO_STT_BIN` overrides (tests/dev);
/// otherwise it sits next to the app executable (Tauri externalBin layout).
pub fn sidecar_path() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("ARISO_STT_BIN") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe.parent().ok_or("no parent dir for current_exe")?;
    Ok(dir.join("ariso-stt"))
}

/// Run the sidecar in transcribe mode and parse its JSON stdout.
pub async fn run_transcribe(audio: &Path, models: &Path) -> Result<TranscriptResult, String> {
    let bin = sidecar_path()?;
    let output = Command::new(&bin)
        .arg("--audio")
        .arg(audio)
        .arg("--models")
        .arg(models)
        .arg("--format")
        .arg("json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ariso-stt failed: {}", stderr.trim()));
    }
    serde_json::from_slice::<TranscriptResult>(&output.stdout).map_err(|e| {
        // Include a bounded, char-safe preview of stdout for diagnosis.
        let preview: String = String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(200)
            .collect();
        format!("parse transcript json: {e} (stdout: {preview})")
    })
}

/// Run the sidecar in notes mode and return the generated markdown (stdout).
/// Bounded by [`NOTES_TIMEOUT`]; on timeout the child process is killed
/// (via `kill_on_drop`) so a hung sidecar can't keep a caller pending.
pub async fn run_notes(transcript: &Path, models: &Path) -> Result<String, String> {
    let bin = sidecar_path()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("notes")
        .arg("--transcript")
        .arg(transcript)
        .arg("--models")
        .arg(models)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(NOTES_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            format!(
                "ariso-stt notes timed out after {}s",
                NOTES_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ariso-stt notes failed: {}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Whether the transcript's on-disk bytes differ between two reads. Used to
/// detect that a later append/regeneration superseded an in-flight notes run.
fn transcript_changed(before: &Option<Vec<u8>>, after: &Option<Vec<u8>>) -> bool {
    before != after
}

/// Best-effort notes generation: runs the sidecar and writes either
/// `ari-note.md` or `meta.notes_error`. Failures here never affect the
/// recording's `Done` status. A third outcome exists: if the transcript
/// changes mid-run (superseded by a later append/regeneration), this writes
/// neither file and silently discards its result.
async fn process_notes(dir: PathBuf, models: PathBuf, mut meta: RecordingMeta) {
    let transcript_path = dir.join("transcript.md");
    // Capture the transcript this run generates from; if it changes while notes
    // run (a later append/regeneration), a newer run owns the result — discard.
    let before = std::fs::read(&transcript_path).ok();
    let outcome = run_notes(&transcript_path, &models).await;
    if transcript_changed(&before, &std::fs::read(&transcript_path).ok()) {
        return;
    }
    match outcome {
        // Empty output is a silent failure: it would write a blank
        // ari-note.md with notes_error unset, reading as success. Record it.
        Ok(notes) if notes.trim().is_empty() => {
            eprintln!("notes generation: empty output");
            meta.notes_error = Some("notes generation produced empty output".to_string());
            let _ = storage::write_meta(&dir, &meta);
        }
        Ok(notes) => {
            if let Err(e) = storage::write_notes(&dir, &notes) {
                eprintln!("write notes: {e}");
                meta.notes_error = Some(e);
                let _ = storage::write_meta(&dir, &meta);
            }
        }
        Err(e) => {
            eprintln!("notes generation: {e}");
            meta.notes_error = Some(e);
            let _ = storage::write_meta(&dir, &meta);
        }
    }
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

    // Persist the audio first so it is never lost, even if STT fails.
    std::fs::write(dir.join("recording.mp3"), &audio).map_err(|e| format!("write audio: {e}"))?;

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
        audio_file: None,
        notes_written: None,
    };
    storage::write_meta(&dir, &meta)?;

    let models = storage::models_dir(root);
    let audio_path = dir.join("recording.mp3");
    match run_transcribe(&audio_path, &models).await {
        Ok(result) => {
            meta.language = Some(result.language.clone());
            meta.participants = result.participants.clone();
            meta.model_version = Some(storage::MODEL_VERSION.to_string());
            storage::write_segments(&dir, &storage::SegmentsFile {
                language: Some(result.language.clone()),
                participants: result.participants.clone(),
                segments: result.segments.clone(),
            })?;
            let md = storage::render_markdown(&meta, &result.segments);
            storage::write_transcript(&dir, &md)?;
            meta.status = RecordingStatus::Done;
            storage::write_meta(&dir, &meta)?;

            // Spawn notes generation detached: it's best-effort and writes
            // its outcome (ari-note.md or meta.notes_error) directly to disk,
            // so the UI/library state never has to wait on it.
            let notes_handle = tokio::spawn(process_notes(dir.clone(), models, meta.clone()));

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
            if let Err(e) = std::fs::write(dir.join("recording.mp3"), audio) {
                eprintln!("save failed clip audio: {e}");
            }
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
                audio_file: None,
                notes_written: None,
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
    let _append_guard = get_append_lock(target_id).lock_owned().await;

    // Re-read meta/segments inside the lock. A concurrent append may have
    // completed (and changed status) since finalize_core's
    // most_recent_appendable check. Fall back to a fresh recording if the
    // target's state is no longer suitable for an append.
    let (mut meta, mut existing) = match (storage::read_meta(&dir), storage::read_segments(&dir)) {
        (Ok(m), Ok(Some(s))) if m.status == storage::RecordingStatus::Done => (m, s),
        _ => return fresh_recording_core(root, audio, title, created_at, duration_seconds).await,
    };

    // Capture the pre-append duration as the new clip's time offset before
    // `meta` is mutated any further below.
    let time_offset = meta.duration_seconds as f64;

    let models = storage::models_dir(root);
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
    // then the rendered transcript.
    let mut combined = std::fs::read(dir.join("recording.mp3")).map_err(|e| format!("read audio: {e}"))?;
    combined.extend_from_slice(&audio);
    storage::write_atomic(&dir.join("recording.mp3"), &combined)?;
    storage::write_segments(&dir, &existing)?;
    let md = storage::render_markdown(&meta, &existing.segments);
    storage::write_transcript(&dir, &md)?;

    // Clear any stale notes_error from a prior failed attempt (mirrors
    // `retry_notes_core`) and commit: status Done, written last.
    meta.notes_error = None;
    meta.status = RecordingStatus::Done;
    storage::write_meta(&dir, &meta)?;
    let _ = std::fs::remove_file(&clip_path);

    let notes_handle = tokio::spawn(process_notes(dir.clone(), models, meta.clone()));
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
    let audio = std::fs::read(dir.join("recording.mp3"))
        .map_err(|e| format!("read recording audio: {e}"))?;
    finalize_core(root, audio, meta.title, meta.created_at, meta.duration_seconds).await
}

/// Regenerate AI notes for a recording from its existing `transcript.md`,
/// without re-running STT. Clears any prior `notes_error` and removes any
/// existing `ari-note.md` first — readiness is signaled by that file's
/// presence, so leaving a stale note in place would make the poller report the
/// recording as "ready" the instant polling resumes, finishing the regeneration
/// silently in the background. Removing it makes the regeneration observable
/// (`hasNote` false → generating → true when the new note lands). Then spawns
/// `process_notes` detached (it writes `ari-note.md` or a fresh `notes_error`).
/// Returns the handle so tests can await completion; the command drops it.
///
/// Trade-off: a failed regeneration leaves the recording note-less (surfaced as
/// "AI Notes failed" with a Retry), since the prior note is not restored.
pub async fn retry_notes_core(root: &Path, id: &str) -> Result<JoinHandle<()>, String> {
    storage::validate_recording_id(id)?;
    let dir = storage::recordings_dir(root).join(id);
    if !dir.join("transcript.md").is_file() {
        return Err("no transcript available to generate notes".to_string());
    }
    let mut meta = storage::read_meta(&dir)?;
    meta.notes_error = None;
    storage::write_meta(&dir, &meta)?;
    // Clear the stale note so regeneration is observable (see doc comment).
    // A missing file is fine — only surface real removal errors.
    match std::fs::remove_file(dir.join("ari-note.md")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove stale note: {e}")),
    }
    let models = storage::models_dir(root);
    Ok(tokio::spawn(process_notes(dir, models, meta)))
}

/// Retry transcription (and notes) for a failed local recording.
#[tauri::command]
pub async fn retry_local_transcription(id: String) -> Result<FinalizeResult, String> {
    let root = storage::ariso_root()?;
    // Drop the notes JoinHandle: notes are best-effort and write their outcome
    // to meta.json directly, matching `local_finalize_recording`.
    retry_transcription_core(&root, &id)
        .await
        .map(|(res, _notes)| res)
}

/// Retry only AI-notes generation for a recording whose transcript exists.
#[tauri::command]
pub async fn retry_local_notes(id: String) -> Result<(), String> {
    let root = storage::ariso_root()?;
    // Drop the JoinHandle: the detached task writes its outcome to disk; the
    // frontend observes completion via `local_recording_status` polling.
    retry_notes_core(&root, &id).await.map(|_handle| ())
}

/// Resolve the recording id a new local recording starting at `created_at` will
/// finalize into — the append target (if it will merge into the recent
/// recording) or the new recording's own id. The recorder calls this at start so
/// the library shows the correct row (the current meeting on a resume, a new row
/// otherwise) instead of a phantom new note that vanishes when the append lands.
#[tauri::command]
pub async fn local_recording_id_for_start(created_at: String) -> Result<String, String> {
    let root = storage::ariso_root()?;
    storage::resolve_local_recording_id(&root, &created_at)
}

#[tauri::command]
pub async fn local_finalize_recording(
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<FinalizeResult, String> {
    let root = storage::ariso_root()?;
    // Drop the notes JoinHandle: notes are best-effort and continue running
    // in the background, writing their outcome to meta.json directly.
    finalize_core(&root, audio, title, created_at, duration_seconds)
        .await
        .map(|(res, _notes)| res)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // SAFETY (all set_var/remove_var below): tests run with `--test-threads=1`,
    // so there is no concurrent env mutation while these calls execute.

    /// Write an executable stub script and point ARISO_STT_BIN at it.
    fn write_stub(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("stub-stt.sh");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "#!/bin/sh\n{body}").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[tokio::test]
    async fn parses_stub_transcript_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        let stub = write_stub(tmp.path(), &format!("cat <<'EOF'\n{json}\nEOF"));
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
        let stub = write_stub(tmp.path(), "echo 'boom' >&2\nexit 1");
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
        let json = r#"{"language":"en","durationSeconds":12.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        // Branch on the `notes` subcommand so the stub's transcript JSON isn't
        // dumped into ari-note.md; this test exercises only the transcribe path.
        let body =
            format!("if [ \"$1\" = notes ]; then echo '# Notes'; exit 0; fi\ncat <<'EOF'\n{json}\nEOF");
        let stub = write_stub(tmp.path(), &body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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
        assert!(dir.join("recording.mp3").exists());
        assert!(dir.join("transcript.md").exists());
        assert_eq!(read_meta(&dir).unwrap().status, RecordingStatus::Done);
    }

    #[tokio::test]
    async fn finalize_writes_segments_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        let body = format!("if [ \"$1\" = notes ]; then echo '# Notes'; exit 0; fi\ncat <<'EOF'\n{json}\nEOF");
        let stub = write_stub(tmp.path(), &body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05.000Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let seg = crate::storage::read_segments(&dir).unwrap().expect("segments.json written");
        assert_eq!(seg.segments.len(), 1);
        assert_eq!(seg.participants[0].label, "Speaker 1");
    }

    #[tokio::test]
    async fn finalize_marks_failed_but_keeps_audio_on_stt_error() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(tmp.path(), "echo 'boom' >&2\nexit 1");
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let err = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05Z".into(), 5,
        ).await.unwrap_err();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert!(err.contains("boom"), "got: {err}");
        let dir = crate::storage::recordings_dir(tmp.path()).join("2026-06-02T14-30-05Z");
        assert!(dir.join("recording.mp3").exists(), "audio must be retained");
        let meta = read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Failed);
        assert!(meta.error.is_some());
    }

    #[tokio::test]
    async fn finalize_writes_ari_note_md_when_notes_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        // Stub: `notes` subcommand prints markdown; otherwise print transcript JSON.
        let body = format!(
            "if [ \"$1\" = notes ]; then echo '# Notes'; echo '- did a thing'; exit 0; fi\ncat <<'EOF'\n{json}\nEOF"
        );
        let stub = write_stub(tmp.path(), &body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, notes_handle) = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "T".into(), "2026-06-02T14:30:05Z".into(), 12,
        ).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert_eq!(res.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let notes = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert!(notes.contains("# Notes"), "got: {notes}");
        assert!(crate::storage::read_meta(&dir).unwrap().notes_error.is_none());
    }

    #[tokio::test]
    async fn finalize_stays_done_when_notes_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"{"language":"en","durationSeconds":12.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        // Stub: `notes` subcommand fails; transcribe still succeeds.
        let body = format!(
            "if [ \"$1\" = notes ]; then echo 'notes boom' >&2; exit 1; fi\ncat <<'EOF'\n{json}\nEOF"
        );
        let stub = write_stub(tmp.path(), &body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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
        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Done);
        assert!(meta.notes_error.is_some());
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
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Stub: notes subcommand prints markdown; otherwise transcript JSON.
        let json = r#"{"language":"en","durationSeconds":5.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"hi","start":0.0,"end":1.0}]}"#;
        let body = format!(
            "if [ \"$1\" = notes ]; then echo '# Notes'; exit 0; fi\ncat <<'EOF'\n{json}\nEOF"
        );
        let stub = write_stub(tmp.path(), &body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let (res, notes_handle) = retry_transcription_core(root, id).await.unwrap();
        notes_handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

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
        };
        storage::write_meta(&dir, &meta).unwrap();

        let err = retry_transcription_core(root, id).await.unwrap_err();
        assert!(err.contains("recording audio"), "got: {err}");
    }

    #[tokio::test]
    async fn retry_notes_regenerates_from_existing_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"# Transcript\nhi").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None,
            notes_error: Some("prior notes failure".into()),
            last_clip_end_at: None, audio_file: None, notes_written: None,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Stub: notes subcommand succeeds.
        let body = "if [ \"$1\" = notes ]; then echo '# Notes'; echo '- point'; exit 0; fi\nexit 1";
        let stub = write_stub(tmp.path(), body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let handle = retry_notes_core(root, id).await.unwrap();
        handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        let notes = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert!(notes.contains("# Notes"), "got: {notes}");
        assert!(read_meta(&dir).unwrap().notes_error.is_none(), "notes_error must be cleared");
    }

    #[tokio::test]
    async fn retry_notes_removes_stale_note_so_regeneration_is_observable() {
        // Regenerate precondition: a note already exists. The readiness signal is
        // `ari-note.md`'s presence, so retry must remove the stale note up front —
        // otherwise the poller would instantly report the old note as "ready" and
        // the regeneration would finish in the background, never surfacing.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"# Transcript\nhi").unwrap();
        std::fs::write(dir.join("ari-note.md"), b"# Old note").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Notes regeneration fails: with the stale note cleared up front, the
        // recording is observably note-less afterwards (rather than still showing
        // the old note), which is what lets the poller track regeneration.
        let body = "if [ \"$1\" = notes ]; then echo 'boom' >&2; exit 1; fi\nexit 1";
        let stub = write_stub(tmp.path(), body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let handle = retry_notes_core(root, id).await.unwrap();
        handle.await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert!(
            !dir.join("ari-note.md").exists(),
            "stale note must be removed so regeneration is observable"
        );
        assert!(read_meta(&dir).unwrap().notes_error.is_some());
    }

    // Helper: a stub that prints notes for `notes`, else a single-segment JSON
    // whose text is the first CLI arg's file contents is overkill — instead emit a
    // fixed clip transcript. Each call yields one segment "clip".
    fn clip_stub(dir: &Path) -> PathBuf {
        let json = r#"{"language":"en","participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"clip text","start":0.0,"end":3.0}]}"#;
        let body = format!("if [ \"$1\" = notes ]; then echo '# Notes'; exit 0; fi\ncat <<'EOF'\n{json}\nEOF");
        write_stub(dir, &body)
    }

    #[tokio::test]
    async fn second_recording_within_window_appends_to_first() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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
        assert_eq!(std::fs::read(dir.join("recording.mp3")).unwrap(), b"aaabbb");
        // transcript.md re-rendered with both clips.
        let md = std::fs::read_to_string(dir.join("transcript.md")).unwrap();
        assert_eq!(md.matches("clip text").count(), 2);
        // Only one recording directory exists.
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn second_recording_outside_window_is_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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

        assert_ne!(r2.id, r1.id);
        let count = std::fs::read_dir(crate::storage::recordings_dir(tmp.path())).unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir()).count();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn append_stt_failure_leaves_target_untouched_and_saves_failed_clip() {
        let tmp = tempfile::tempdir().unwrap();
        // First: success stub.
        let ok_stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &ok_stub); }
        let (r1, h1) = finalize_core(
            tmp.path(), b"aaa".to_vec(), "T".into(), "2026-06-02T10:00:00.000Z".into(), 30,
        ).await.unwrap();
        h1.await.unwrap();

        // Second: failing stub → append must fall back to a separate Failed recording.
        let fail_stub = write_stub(tmp.path(), "echo 'boom' >&2\nexit 1");
        unsafe { std::env::set_var("ARISO_STT_BIN", &fail_stub); }
        let err = finalize_core(
            tmp.path(), b"bbb".to_vec(), "T2".into(), "2026-06-02T10:01:00.000Z".into(), 15,
        ).await.unwrap_err();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }
        assert!(err.contains("boom"), "got: {err}");

        // Target untouched: still one segment, 30s, audio "aaa".
        let dir1 = crate::storage::recordings_dir(tmp.path()).join(&r1.id);
        assert_eq!(crate::storage::read_segments(&dir1).unwrap().unwrap().segments.len(), 1);
        assert_eq!(crate::storage::read_meta(&dir1).unwrap().duration_seconds, 30);
        assert_eq!(std::fs::read(dir1.join("recording.mp3")).unwrap(), b"aaa");

        // The failed clip is its own recording, audio retained, status Failed.
        let dir2 = crate::storage::recordings_dir(tmp.path()).join("2026-06-02T10-01-00Z");
        assert!(dir2.join("recording.mp3").exists());
        assert_eq!(crate::storage::read_meta(&dir2).unwrap().status, RecordingStatus::Failed);
        assert!(!dir1.join("append-clip.mp3").exists(), "temp clip cleaned up");
    }

    #[tokio::test]
    async fn append_falls_back_to_fresh_when_target_segments_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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

        // The clip's audio is preserved as its own fresh recording.
        let dir2 = crate::storage::recordings_dir(tmp.path()).join(&r2.id);
        assert_eq!(std::fs::read(dir2.join("recording.mp3")).unwrap(), b"bbb");
        assert_eq!(crate::storage::read_meta(&dir2).unwrap().status, RecordingStatus::Done);
    }

    #[tokio::test]
    async fn append_clears_prior_notes_error() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = clip_stub(tmp.path());
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

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
        };
        storage::write_meta(&dir, &meta).unwrap();

        let err = retry_notes_core(root, id).await.unwrap_err();
        assert!(err.contains("transcript"), "got: {err}");
    }

    #[test]
    fn transcript_changed_detects_difference() {
        assert!(!transcript_changed(&Some(b"a".to_vec()), &Some(b"a".to_vec())));
        assert!(transcript_changed(&Some(b"a".to_vec()), &Some(b"b".to_vec())));
        assert!(transcript_changed(&Some(b"a".to_vec()), &None));
    }

    #[tokio::test]
    async fn notes_discarded_when_transcript_changes_midflight() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T10-00-00Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        std::fs::write(dir.join("transcript.md"), b"original").unwrap();
        let meta = RecordingMeta {
            id: id.into(), title: "T".into(), created_at: "2026-06-02T10:00:00Z".into(),
            duration_seconds: 5, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
            last_clip_end_at: None, audio_file: None, notes_written: None,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Notes stub REWRITES the transcript it was handed (simulating an append that
        // landed while notes were generating), then prints notes. process_notes must
        // then see the change and discard its output.
        let body = "if [ \"$1\" = notes ]; then echo 'changed' > \"$3\"; echo '# Notes'; exit 0; fi\nexit 1";
        let stub = write_stub(root, body);
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        // process_notes takes (dir, models, meta). $3 is the --transcript path arg.
        process_notes(dir.clone(), storage::models_dir(root), meta.clone()).await;
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

        assert!(!dir.join("ari-note.md").exists(), "superseded notes must be discarded");
        assert!(storage::read_meta(&dir).unwrap().notes_error.is_none(), "no stale error written");
    }
}
