use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::task::JoinHandle;

use crate::transcript_normalization::{
    SidecarTranscriptResult, TranscriptResult, normalize_transcript,
};

/// Upper bound on how long the notes sidecar may run before we kill it.
/// Notes generation is best-effort and runs detached from `finalize_core`,
/// so this only bounds the background task's lifetime.
const NOTES_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeResult {
    pub backend: String,
    pub id: String,
    pub title: String,
    pub status: crate::storage::RecordingStatus,
}

/// Resolve the platform's `ariso-stt` sidecar from Tauri's externalBin layout.
/// Engine selection does not leak beyond the common CLI and JSON contract.
pub fn sidecar_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe.parent().ok_or("no parent dir for current_exe")?;
    Ok(dir.join(if cfg!(target_os = "windows") {
        "ariso-stt.exe"
    } else {
        "ariso-stt"
    }))
}

/// Run the sidecar in transcribe mode and parse its JSON stdout.
async fn run_transcribe_with_sidecar(
    bin: &Path,
    audio: &Path,
    models: &Path,
) -> Result<TranscriptResult, String> {
    let output = Command::new(bin)
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

/// Run the sidecar in notes mode and return the generated markdown (stdout).
/// Bounded by [`NOTES_TIMEOUT`]; on timeout the child process is killed
/// (via `kill_on_drop`) so a hung sidecar can't keep a caller pending.
async fn run_notes_with_sidecar(
    bin: &Path,
    transcript: &Path,
    models: &Path,
) -> Result<String, String> {
    let mut cmd = Command::new(bin);
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

/// Best-effort notes generation: runs the sidecar and writes either
/// `ari-note.md` or `meta.notes_error`. Failures here never affect the
/// recording's `Done` status.
async fn process_notes(sidecar: PathBuf, dir: PathBuf, models: PathBuf, mut meta: RecordingMeta) {
    let transcript_path = dir.join("transcript.md");
    match run_notes_with_sidecar(&sidecar, &transcript_path, &models).await {
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
    let sidecar = sidecar_path()?;
    finalize_core_with_sidecar(root, audio, title, created_at, duration_seconds, &sidecar).await
}

async fn finalize_core_with_sidecar(
    root: &Path,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
    sidecar: &Path,
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
    };
    storage::write_meta(&dir, &meta)?;

    let models = storage::models_dir(root);
    let audio_path = dir.join("recording.mp3");
    match run_transcribe_with_sidecar(sidecar, &audio_path, &models).await {
        Ok(result) => {
            meta.language = Some(result.language.clone());
            meta.participants = result.participants.clone();
            meta.model_version = Some(crate::model_manager::stt_model_version());
            let md = storage::render_markdown(&meta, &result.segments);
            storage::write_transcript(&dir, &md)?;
            meta.status = RecordingStatus::Done;
            storage::write_meta(&dir, &meta)?;

            // Spawn notes generation detached: it's best-effort and writes
            // its outcome (ari-note.md or meta.notes_error) directly to disk,
            // so the UI/library state never has to wait on it.
            let notes_handle = tokio::spawn(process_notes(
                sidecar.to_path_buf(),
                dir.clone(),
                models,
                meta.clone(),
            ));

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

/// Re-run the full pipeline (transcription + notes) for an existing recording,
/// reusing its saved `recording.mp3`. `finalize_core` derives the recording's
/// dir from `created_at`, which equals the existing folder id, so it rewrites
/// in place and resets `status`/`error`/`notes_error`. Returns the result plus
/// the detached notes `JoinHandle` (tests await it; the command drops it).
pub async fn retry_transcription_core(
    root: &Path,
    id: &str,
) -> Result<(FinalizeResult, JoinHandle<()>), String> {
    let sidecar = sidecar_path()?;
    retry_transcription_core_with_sidecar(root, id, &sidecar).await
}

async fn retry_transcription_core_with_sidecar(
    root: &Path,
    id: &str,
    sidecar: &Path,
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
    finalize_core_with_sidecar(
        root,
        audio,
        meta.title,
        meta.created_at,
        meta.duration_seconds,
        sidecar,
    )
    .await
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
    let sidecar = sidecar_path()?;
    retry_notes_core_with_sidecar(root, id, &sidecar).await
}

async fn retry_notes_core_with_sidecar(
    root: &Path,
    id: &str,
    sidecar: &Path,
) -> Result<JoinHandle<()>, String> {
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
    Ok(tokio::spawn(process_notes(
        sidecar.to_path_buf(),
        dir,
        models,
        meta,
    )))
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

#[cfg(test)]
mod tests {
    use super::*;

    const SHARED_TRANSCRIPT_FIXTURE: &str =
        include_str!("../ariso-stt/shared/fixtures/transcript.json");
    const TRANSCRIBE_PAYLOAD: &str = "stub-transcribe.txt";
    const NOTES_PAYLOAD: &str = "stub-notes.txt";

    #[derive(Clone, Copy)]
    enum StubOutcome<'a> {
        Success(&'a str),
        Failure(&'a str),
    }

    #[derive(Clone, Copy)]
    struct StubBehavior<'a> {
        transcribe: StubOutcome<'a>,
        notes: StubOutcome<'a>,
    }

    impl<'a> StubBehavior<'a> {
        fn transcribe_success(output: &'a str) -> Self {
            Self {
                transcribe: StubOutcome::Success(output),
                notes: StubOutcome::Failure("unexpected notes invocation"),
            }
        }

        fn transcribe_failure(error: &'a str) -> Self {
            Self {
                transcribe: StubOutcome::Failure(error),
                notes: StubOutcome::Failure("unexpected notes invocation"),
            }
        }

        fn with_notes(mut self, notes: StubOutcome<'a>) -> Self {
            self.notes = notes;
            self
        }
    }

    fn outcome_payload(outcome: StubOutcome<'_>) -> &str {
        match outcome {
            StubOutcome::Success(payload) | StubOutcome::Failure(payload) => payload,
        }
    }

    fn outcome_exit_code(outcome: StubOutcome<'_>) -> u8 {
        match outcome {
            StubOutcome::Success(_) => 0,
            StubOutcome::Failure(_) => 1,
        }
    }

    fn outcome_redirect(outcome: StubOutcome<'_>) -> &'static str {
        match outcome {
            StubOutcome::Success(_) => "",
            StubOutcome::Failure(_) => " 1>&2",
        }
    }

    /// Write an executable stub implementing the sidecar process contract.
    fn write_stub(dir: &Path, behavior: StubBehavior<'_>) -> PathBuf {
        let path = if cfg!(target_os = "windows") {
            dir.join("stub-stt.cmd")
        } else {
            dir.join("stub-stt.sh")
        };
        std::fs::write(
            dir.join(TRANSCRIBE_PAYLOAD),
            outcome_payload(behavior.transcribe),
        )
        .unwrap();
        std::fs::write(dir.join(NOTES_PAYLOAD), outcome_payload(behavior.notes)).unwrap();
        std::fs::write(&path, platform_stub_body(behavior)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    #[cfg(unix)]
    fn platform_stub_body(behavior: StubBehavior<'_>) -> String {
        format!(
            "#!/bin/sh\nif [ \"$1\" = notes ]; then\n  cat \"$(dirname \"$0\")/{NOTES_PAYLOAD}\"{}\n  exit {}\nfi\ncat \"$(dirname \"$0\")/{TRANSCRIBE_PAYLOAD}\"{}\nexit {}\n",
            outcome_redirect(behavior.notes),
            outcome_exit_code(behavior.notes),
            outcome_redirect(behavior.transcribe),
            outcome_exit_code(behavior.transcribe),
        )
    }

    #[cfg(windows)]
    fn platform_stub_body(behavior: StubBehavior<'_>) -> String {
        format!(
            "@echo off\r\nif \"%1\"==\"notes\" (\r\n  type \"%~dp0{NOTES_PAYLOAD}\"{}\r\n  exit /b {}\r\n)\r\ntype \"%~dp0{TRANSCRIBE_PAYLOAD}\"{}\r\nexit /b {}\r\n",
            outcome_redirect(behavior.notes),
            outcome_exit_code(behavior.notes),
            outcome_redirect(behavior.transcribe),
            outcome_exit_code(behavior.transcribe),
        )
    }

    #[tokio::test]
    async fn parses_stub_transcript_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = SHARED_TRANSCRIPT_FIXTURE;
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_success(json));

        let audio = tmp.path().join("a.mp3");
        std::fs::write(&audio, b"x").unwrap();
        let res = run_transcribe_with_sidecar(&stub, &audio, tmp.path())
            .await
            .unwrap();

        assert_eq!(res.language, "en");
        assert_eq!(res.segments.len(), 1);
        assert_eq!(res.participants[0].label, "Speaker 1");
    }

    #[tokio::test]
    async fn surfaces_stub_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_failure("boom"));
        let audio = tmp.path().join("a.mp3");
        std::fs::write(&audio, b"x").unwrap();
        let err = run_transcribe_with_sidecar(&stub, &audio, tmp.path())
            .await
            .unwrap_err();
        assert!(err.contains("boom"), "got: {err}");
    }

    use crate::storage::{RecordingStatus, read_meta};

    #[tokio::test]
    async fn finalize_writes_transcript_and_marks_done() {
        let tmp = tempfile::tempdir().unwrap();
        let json = SHARED_TRANSCRIPT_FIXTURE;
        // Branch on the `notes` subcommand so the stub's transcript JSON isn't
        // dumped into ari-note.md; this test exercises only the transcribe path.
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json).with_notes(StubOutcome::Success("# Notes")),
        );

        let (res, notes_handle) = finalize_core_with_sidecar(
            tmp.path(),
            b"audio".to_vec(),
            "My Title".into(),
            "2026-06-02T14:30:05.000Z".into(),
            12,
            &stub,
        )
        .await
        .unwrap();
        notes_handle.await.unwrap();

        assert_eq!(res.status, RecordingStatus::Done);
        assert_eq!(res.id, "2026-06-02T14-30-05Z");
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        assert!(dir.join("recording.mp3").exists());
        assert!(dir.join("transcript.md").exists());
        assert_eq!(read_meta(&dir).unwrap().status, RecordingStatus::Done);
    }

    #[tokio::test]
    async fn finalize_marks_failed_but_keeps_audio_on_stt_error() {
        let tmp = tempfile::tempdir().unwrap();
        let stub = write_stub(tmp.path(), StubBehavior::transcribe_failure("boom"));

        let err = finalize_core_with_sidecar(
            tmp.path(),
            b"audio".to_vec(),
            "T".into(),
            "2026-06-02T14:30:05Z".into(),
            5,
            &stub,
        )
        .await
        .unwrap_err();

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
        let json = SHARED_TRANSCRIPT_FIXTURE;
        // Stub: `notes` subcommand prints markdown; otherwise print transcript JSON.
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json).with_notes(StubOutcome::Success(
                "# Notes\n- 100% ready & reviewed ^ approved",
            )),
        );

        let (res, notes_handle) = finalize_core_with_sidecar(
            tmp.path(),
            b"audio".to_vec(),
            "T".into(),
            "2026-06-02T14:30:05Z".into(),
            12,
            &stub,
        )
        .await
        .unwrap();
        notes_handle.await.unwrap();

        assert_eq!(res.status, RecordingStatus::Done);
        let dir = crate::storage::recordings_dir(tmp.path()).join(&res.id);
        let notes = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert!(notes.contains("# Notes"), "got: {notes}");
        assert!(
            notes.contains("100% ready & reviewed ^ approved"),
            "stub payload must survive shell metacharacters: {notes}"
        );
        assert!(
            crate::storage::read_meta(&dir)
                .unwrap()
                .notes_error
                .is_none()
        );
    }

    #[tokio::test]
    async fn finalize_stays_done_when_notes_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let json = SHARED_TRANSCRIPT_FIXTURE;
        // Stub: `notes` subcommand fails; transcribe still succeeds.
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json).with_notes(StubOutcome::Failure("notes boom")),
        );

        let (res, notes_handle) = finalize_core_with_sidecar(
            tmp.path(),
            b"audio".to_vec(),
            "T".into(),
            "2026-06-02T14:30:05Z".into(),
            12,
            &stub,
        )
        .await
        .unwrap();
        notes_handle.await.unwrap();

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
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Stub: notes subcommand prints markdown; otherwise transcript JSON.
        let json = SHARED_TRANSCRIPT_FIXTURE;
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_success(json).with_notes(StubOutcome::Success("# Notes")),
        );

        let (res, notes_handle) = retry_transcription_core_with_sidecar(root, id, &stub)
            .await
            .unwrap();
        notes_handle.await.unwrap();

        assert_eq!(res.status, RecordingStatus::Done);
        assert!(dir.join("transcript.md").exists());
        let meta = read_meta(&dir).unwrap();
        assert_eq!(meta.status, RecordingStatus::Done);
        assert!(
            meta.error.is_none(),
            "prior transcription error must be cleared"
        );
    }

    #[tokio::test]
    async fn retry_transcription_errors_when_audio_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        // meta but no recording.mp3
        let meta = RecordingMeta {
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: RecordingStatus::Failed,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
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
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: Some("prior notes failure".into()),
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Stub: notes subcommand succeeds.
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_failure("unexpected transcription invocation")
                .with_notes(StubOutcome::Success("# Notes\n- point")),
        );

        let handle = retry_notes_core_with_sidecar(root, id, &stub)
            .await
            .unwrap();
        handle.await.unwrap();

        let notes = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert!(notes.contains("# Notes"), "got: {notes}");
        assert!(
            read_meta(&dir).unwrap().notes_error.is_none(),
            "notes_error must be cleared"
        );
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
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
        };
        storage::write_meta(&dir, &meta).unwrap();

        // Notes regeneration fails: with the stale note cleared up front, the
        // recording is observably note-less afterwards (rather than still showing
        // the old note), which is what lets the poller track regeneration.
        let stub = write_stub(
            tmp.path(),
            StubBehavior::transcribe_failure("unexpected transcription invocation")
                .with_notes(StubOutcome::Failure("boom")),
        );

        let handle = retry_notes_core_with_sidecar(root, id, &stub)
            .await
            .unwrap();
        handle.await.unwrap();

        assert!(
            !dir.join("ari-note.md").exists(),
            "stale note must be removed so regeneration is observable"
        );
        assert!(read_meta(&dir).unwrap().notes_error.is_some());
    }

    #[tokio::test]
    async fn retry_notes_errors_without_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = storage::create_recording_dir(root, id).unwrap();
        let meta = RecordingMeta {
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
        };
        storage::write_meta(&dir, &meta).unwrap();

        let err = retry_notes_core(root, id).await.unwrap_err();
        assert!(err.contains("transcript"), "got: {err}");
    }
}
