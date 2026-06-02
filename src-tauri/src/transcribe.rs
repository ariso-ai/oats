use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptResult {
    pub language: String,
    pub duration_seconds: f64,
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
    serde_json::from_slice::<TranscriptResult>(&output.stdout)
        .map_err(|e| format!("parse transcript json: {e}"))
}

use crate::storage::{self, RecordingMeta, RecordingStatus};
use std::path::Path as StdPath;

/// Pure-ish orchestration over an explicit root, so tests use a tempdir.
pub async fn finalize_core(
    root: &StdPath,
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<FinalizeResult, String> {
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
    };
    storage::write_meta(&dir, &meta)?;

    let models = storage::models_dir(root);
    let audio_path = dir.join("recording.mp3");
    match run_transcribe(&audio_path, &models).await {
        Ok(result) => {
            meta.language = Some(result.language.clone());
            meta.participants = result.participants.clone();
            meta.model_version = Some("parakeet-tdt-0.6b-v3".to_string());
            let md = storage::render_markdown(&meta, &result.segments);
            storage::write_transcript(&dir, &md)?;
            meta.status = RecordingStatus::Done;
            storage::write_meta(&dir, &meta)?;
            Ok(FinalizeResult {
                backend: "local".to_string(),
                id,
                title,
                status: RecordingStatus::Done,
            })
        }
        Err(e) => {
            meta.status = RecordingStatus::Failed;
            meta.error = Some(e.clone());
            let _ = storage::write_meta(&dir, &meta);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn local_finalize_recording(
    audio: Vec<u8>,
    title: String,
    created_at: String,
    duration_seconds: u64,
) -> Result<FinalizeResult, String> {
    let root = storage::ariso_root()?;
    finalize_core(&root, audio, title, created_at, duration_seconds).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

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
        let stub = write_stub(tmp.path(), &format!("cat <<'EOF'\n{json}\nEOF"));
        unsafe { std::env::set_var("ARISO_STT_BIN", &stub); }

        let res = finalize_core(
            tmp.path(), b"audio".to_vec(),
            "My Title".into(), "2026-06-02T14:30:05.000Z".into(), 12,
        ).await.unwrap();
        unsafe { std::env::remove_var("ARISO_STT_BIN"); }

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
}
