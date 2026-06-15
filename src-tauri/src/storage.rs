use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Identifier of the on-device STT model, recorded in each recording's
/// `meta.json` and the models `manifest.json`. Single source of truth so the
/// per-recording `modelVersion` and the ready-marker can never drift apart.
pub const MODEL_VERSION: &str = "parakeet-tdt-0.6b-v3";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    Recording,
    Transcribing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Participant {
    pub id: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    pub speaker: u32,
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub status: RecordingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub participants: Vec<Participant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub status: RecordingStatus,
    /// Whether `recording.mp3` exists in the recording's directory.
    pub has_audio: bool,
    /// Whether `ari-note.md` exists in the recording's directory.
    pub has_note: bool,
    /// Whether `transcript.md` exists in the recording's directory.
    pub has_transcript: bool,
}

/// Metadata persisted next to a buffered pending upload (`<id>.json`), so a
/// failed Ariso upload can be resumed after the recorder window closes or the
/// app restarts. Keyed on disk by `sanitize_iso_to_pending_id(created_at)`,
/// which preserves sub-second precision so distinct recordings stopped in the
/// same second do not collide.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingUploadMeta {
    /// Raw ISO timestamp used as the buffer key (`startAt ?? endAt`).
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    pub end_at: String,
    pub duration_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<u64>,
}

/// Resolve the `~/.ariso` root. `ARISO_ROOT` overrides (used by tests/dev);
/// otherwise `$HOME/.ariso`. Errors if neither is available.
pub fn ariso_root() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("ARISO_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "neither HOME nor USERPROFILE is set".to_string())?;
    Ok(PathBuf::from(home).join(".ariso"))
}

pub fn models_dir(root: &Path) -> PathBuf {
    root.join("models")
}

pub fn recordings_dir(root: &Path) -> PathBuf {
    root.join("recordings")
}

/// Where Ariso recordings are buffered between "stop" and a confirmed upload.
/// Files here are plain playable mp3s; an orphan left by a crash can be
/// recovered manually from this folder.
pub fn pending_uploads_dir(root: &Path) -> PathBuf {
    root.join("pending-uploads")
}

/// Reject ids that could escape the pending-uploads dir. Mirrors the guard in
/// `commands::recording_dir`; the sanitizer strips `:` but passes `/` and `\`
/// through, so a hostile timestamp must be caught here.
fn validate_pending_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains(':')
        || id.contains("..")
    {
        return Err(format!("invalid pending audio id: {id}"));
    }
    Ok(())
}

fn pending_audio_path(root: &Path, created_at: &str) -> Result<PathBuf, String> {
    let id = sanitize_iso_to_pending_id(created_at);
    validate_pending_id(&id)?;
    Ok(pending_uploads_dir(root).join(format!("{id}.mp3")))
}

fn pending_meta_path(root: &Path, created_at: &str) -> Result<PathBuf, String> {
    let id = sanitize_iso_to_pending_id(created_at);
    validate_pending_id(&id)?;
    Ok(pending_uploads_dir(root).join(format!("{id}.json")))
}

/// Delete a file, treating "already gone" as success.
fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("discard pending file: {e}")),
    }
}

/// Buffer a stopped recording's mp3 under `pending-uploads/<id>.mp3` and its
/// metadata under `pending-uploads/<id>.json`, where the id is the sanitized
/// `meta.created_at`. Audio is written first so a sidecar is never observed
/// without its audio. Returns the id. Re-buffering the same timestamp
/// overwrites both files (the retry path).
pub fn write_pending_audio(
    root: &Path,
    meta: &PendingUploadMeta,
    bytes: &[u8],
) -> Result<String, String> {
    let audio_path = pending_audio_path(root, &meta.created_at)?;
    let meta_path = pending_meta_path(root, &meta.created_at)?;
    fs::create_dir_all(pending_uploads_dir(root))
        .map_err(|e| format!("create pending-uploads dir: {e}"))?;
    write_atomic(&audio_path, bytes)?;
    let json = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    write_atomic(&meta_path, &json)?;
    Ok(sanitize_iso_to_pending_id(&meta.created_at))
}

/// Delete the buffered mp3 and its sidecar for `created_at`. Missing files are
/// not an error — the success path and an explicit dismiss both call this
/// unconditionally.
pub fn discard_pending_audio(root: &Path, created_at: &str) -> Result<(), String> {
    remove_if_exists(&pending_audio_path(root, created_at)?)?;
    remove_if_exists(&pending_meta_path(root, created_at)?)?;
    Ok(())
}

/// Read a buffered pending upload's mp3 bytes (used to combine for resume).
fn read_pending_audio_bytes(root: &Path, created_at: &str) -> Result<Vec<u8>, String> {
    let path = pending_audio_path(root, created_at)?;
    fs::read(&path).map_err(|e| format!("read pending audio: {e}"))
}

/// Concatenate the mp3 bytes for `created_at_keys` in the order given (the
/// caller passes them chronologically). Errors if any key has no buffered
/// audio, or if the running total would exceed `max_bytes`. All clips come
/// from the same in-app encoder, so byte concatenation decodes cleanly.
pub fn combine_pending_audio(
    root: &Path,
    created_at_keys: &[String],
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let mut combined: Vec<u8> = Vec::new();
    for key in created_at_keys {
        let bytes = read_pending_audio_bytes(root, key)?;
        if combined.len() as u64 + bytes.len() as u64 > max_bytes {
            return Err(format!("combined pending audio exceeds {max_bytes} bytes"));
        }
        combined.extend_from_slice(&bytes);
    }
    Ok(combined)
}

/// List buffered pending uploads, chronological (oldest-first). A sidecar is
/// included only when its sibling `.mp3` still exists; unpaired or unparseable
/// files are skipped (an orphan left by a crash stays for manual recovery).
pub fn list_pending_uploads(root: &Path) -> Result<Vec<PendingUploadMeta>, String> {
    let dir = pending_uploads_dir(root);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("read pending-uploads dir: {e}"))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(meta) = serde_json::from_slice::<PendingUploadMeta>(&bytes) else { continue };
        let has_audio = pending_audio_path(root, &meta.created_at)
            .map(|p| p.is_file())
            .unwrap_or(false);
        if has_audio {
            out.push(meta);
        }
    }
    out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(out)
}

/// Turn a UTC ISO-8601 instant into a filesystem-safe, sortable folder id.
/// "2026-06-02T14:30:05.123Z" -> "2026-06-02T14-30-05Z"
///
/// Assumes a UTC (`Z`) timestamp such as `Date.toISOString()` produces. Any
/// `+HH:MM` offset is dropped before sanitizing so it cannot leak `:` into the
/// id (which would corrupt the folder name and the `list_recordings` ordering).
pub fn sanitize_iso_to_id(iso: &str) -> String {
    let iso = iso.split('+').next().unwrap_or(iso);
    let head = match iso.split_once('.') {
        Some((h, _)) => h,
        None => iso.trim_end_matches('Z'),
    };
    format!("{}Z", head.replace(':', "-"))
}

/// Filesystem-safe id for buffered pending uploads. Differs from
/// `sanitize_iso_to_id` by preserving sub-second precision so two recordings
/// stopped within the same second produce distinct keys, while a retry of the
/// same recording (same `created_at`) stays deterministic.
/// "2026-06-02T14:30:05.123Z" -> "2026-06-02T14-30-05.123Z"
/// "2026-06-02T14:30:05Z"     -> "2026-06-02T14-30-05Z"
pub fn sanitize_iso_to_pending_id(iso: &str) -> String {
    let iso = iso.split('+').next().unwrap_or(iso);
    let trimmed = iso.trim_end_matches('Z');
    format!("{}Z", trimmed.replace(':', "-"))
}

/// Format seconds as HH:MM:SS.
pub fn format_hms(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn create_recording_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    let dir = recordings_dir(root).join(id);
    fs::create_dir_all(&dir).map_err(|e| format!("create recording dir: {e}"))?;
    Ok(dir)
}

pub fn write_meta(dir: &Path, meta: &RecordingMeta) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("meta.json"), json.as_bytes())
}

pub fn read_meta(dir: &Path) -> Result<RecordingMeta, String> {
    let bytes = fs::read(dir.join("meta.json")).map_err(|e| format!("read meta: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse meta: {e}"))
}

/// Write to a sibling temp file then rename, so readers never see a partial file.
///
/// The rename is atomic, but the temp path is fixed, so this assumes a single
/// writer per `path` at a time. Concurrent writers to the same path must be
/// serialized by the caller (e.g. the model download command guards against
/// re-entry).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))
}

/// Render a speaker-attributed markdown transcript with YAML front-matter.
pub fn render_markdown(meta: &RecordingMeta, segments: &[Segment]) -> String {
    let label_for = |speaker: u32| -> String {
        meta.participants
            .iter()
            .find(|p| p.id == speaker)
            .map(|p| p.label.clone())
            .unwrap_or_else(|| format!("Speaker {}", speaker + 1))
    };

    let participant_labels: Vec<String> = if meta.participants.is_empty() {
        let mut ids: Vec<u32> = segments.iter().map(|s| s.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(label_for).collect()
    } else {
        meta.participants.iter().map(|p| p.label.clone()).collect()
    };
    let participants_yaml = participant_labels
        .iter()
        .map(|l| format!("\"{l}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: \"{}\"\n", esc(&meta.title)));
    out.push_str(&format!("date: \"{}\"\n", esc(&meta.created_at)));
    out.push_str(&format!("duration: \"{}\"\n", format_hms(meta.duration_seconds as f64)));
    out.push_str(&format!("participants: [{participants_yaml}]\n"));
    out.push_str("---\n\n");

    for seg in segments {
        out.push_str(&format!(
            "**{}** [{}]\n{}\n\n",
            label_for(seg.speaker),
            format_hms(seg.start),
            seg.text.trim()
        ));
    }
    out
}

/// Write the rendered transcript atomically.
pub fn write_transcript(dir: &Path, markdown: &str) -> Result<(), String> {
    write_atomic(&dir.join("transcript.md"), markdown.as_bytes())
}

/// Write the generated meeting overview atomically.
pub fn write_notes(dir: &Path, markdown: &str) -> Result<(), String> {
    write_atomic(&dir.join("ari-note.md"), markdown.as_bytes())
}

/// List all recordings, newest-first by `created_at`. Folders without a
/// readable `meta.json` are skipped. Missing recordings dir => empty list.
pub fn list_recordings(root: &Path) -> Result<Vec<RecordingSummary>, String> {
    let dir = recordings_dir(root);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("read recordings dir: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        match read_meta(&entry.path()) {
            Ok(m) => {
                let recording_dir = entry.path();
                out.push(RecordingSummary {
                    id: m.id,
                    title: m.title,
                    created_at: m.created_at,
                    duration_seconds: m.duration_seconds,
                    status: m.status,
                    has_audio: recording_dir.join("recording.mp3").is_file(),
                    has_note: recording_dir.join("ari-note.md").is_file(),
                    has_transcript: recording_dir.join("transcript.md").is_file(),
                });
            }
            Err(_) => continue,
        }
    }
    // Lexical descending == chronological newest-first, because `created_at`
    // is a consistently-formatted UTC ISO-8601 timestamp.
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pmeta(created_at: &str) -> PendingUploadMeta {
        PendingUploadMeta {
            created_at: created_at.into(),
            start_at: Some(created_at.into()),
            end_at: created_at.into(),
            duration_seconds: 1,
            meeting_id: None,
        }
    }

    #[test]
    fn dirs_derive_from_root() {
        let root = Path::new("/tmp/dotariso");
        assert_eq!(models_dir(root), Path::new("/tmp/dotariso/models"));
        assert_eq!(recordings_dir(root), Path::new("/tmp/dotariso/recordings"));
    }

    #[test]
    fn sanitizes_iso_with_millis() {
        assert_eq!(sanitize_iso_to_id("2026-06-02T14:30:05.123Z"), "2026-06-02T14-30-05Z");
    }

    #[test]
    fn sanitizes_iso_without_millis() {
        assert_eq!(sanitize_iso_to_id("2026-06-02T14:30:05Z"), "2026-06-02T14-30-05Z");
    }

    #[test]
    fn sanitizes_iso_drops_offset() {
        assert_eq!(sanitize_iso_to_id("2026-06-02T14:30:05+00:00"), "2026-06-02T14-30-05Z");
    }

    #[test]
    fn formats_hms() {
        assert_eq!(format_hms(0.0), "00:00:00");
        assert_eq!(format_hms(3.4), "00:00:03");
        assert_eq!(format_hms(2533.0), "00:42:13");
    }

    fn meta_with(id: &str, created: &str) -> RecordingMeta {
        RecordingMeta {
            id: id.into(), title: format!("T {id}"), created_at: created.into(),
            duration_seconds: 1, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
        }
    }

    #[test]
    fn lists_recordings_newest_first_and_skips_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for (id, created) in [
            ("2026-06-01T10-00-00Z", "2026-06-01T10:00:00Z"),
            ("2026-06-02T10-00-00Z", "2026-06-02T10:00:00Z"),
        ] {
            let dir = create_recording_dir(root, id).unwrap();
            write_meta(&dir, &meta_with(id, created)).unwrap();
        }
        std::fs::create_dir_all(recordings_dir(root).join("garbage")).unwrap();

        let list = list_recordings(root).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "2026-06-02T10-00-00Z");
        assert_eq!(list[1].id, "2026-06-01T10-00-00Z");
        // Artifact files absent — all flags should be false.
        assert!(!list[0].has_audio);
        assert!(!list[0].has_note);
        assert!(!list[0].has_transcript);
        assert!(!list[1].has_audio);
        assert!(!list[1].has_note);
        assert!(!list[1].has_transcript);
    }

    #[test]
    fn lists_recordings_artifact_flags_reflect_file_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Recording with all three artifact files present.
        let id_full = "2026-06-03T10-00-00Z";
        let dir_full = create_recording_dir(root, id_full).unwrap();
        write_meta(&dir_full, &meta_with(id_full, "2026-06-03T10:00:00Z")).unwrap();
        std::fs::write(dir_full.join("recording.mp3"), b"audio").unwrap();
        std::fs::write(dir_full.join("ari-note.md"), b"notes").unwrap();
        std::fs::write(dir_full.join("transcript.md"), b"transcript").unwrap();

        // Recording with no artifact files.
        let id_empty = "2026-06-02T10-00-00Z";
        let dir_empty = create_recording_dir(root, id_empty).unwrap();
        write_meta(&dir_empty, &meta_with(id_empty, "2026-06-02T10:00:00Z")).unwrap();

        let list = list_recordings(root).unwrap();
        assert_eq!(list.len(), 2);
        // Newest first: id_full at index 0.
        assert_eq!(list[0].id, id_full);
        assert!(list[0].has_audio);
        assert!(list[0].has_note);
        assert!(list[0].has_transcript);

        assert_eq!(list[1].id, id_empty);
        assert!(!list[1].has_audio);
        assert!(!list[1].has_note);
        assert!(!list[1].has_transcript);
    }

    #[test]
    fn lists_empty_when_no_recordings_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let list = list_recordings(tmp.path()).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn renders_markdown_with_speaker_blocks() {
        let meta = RecordingMeta {
            id: "x".into(),
            title: "Recording 2026-06-02 14:30".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 2533,
            status: RecordingStatus::Done,
            language: Some("en".into()),
            participants: vec![
                Participant { id: 0, label: "Speaker 1".into() },
                Participant { id: 1, label: "Speaker 2".into() },
            ],
            model_version: Some("parakeet-tdt-0.6b-v3".into()),
            error: None,
            notes_error: None,
        };
        let segments = vec![
            Segment { speaker: 0, text: "Hello there".into(), start: 3.0, end: 9.0 },
            Segment { speaker: 1, text: "Hi back".into(), start: 9.0, end: 12.0 },
        ];
        let md = render_markdown(&meta, &segments);

        assert!(md.starts_with("---\n"));
        assert!(md.contains("title: \"Recording 2026-06-02 14:30\""));
        assert!(md.contains("duration: \"00:42:13\""));
        assert!(md.contains("**Speaker 1** [00:00:03]\nHello there"));
        assert!(md.contains("**Speaker 2** [00:00:09]\nHi back"));
    }

    #[test]
    fn unknown_speaker_falls_back_to_label() {
        let meta = RecordingMeta {
            id: "x".into(), title: "t".into(), created_at: "c".into(),
            duration_seconds: 0, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None, notes_error: None,
        };
        let segments = vec![Segment { speaker: 5, text: "hi".into(), start: 0.0, end: 1.0 }];
        let md = render_markdown(&meta, &segments);
        assert!(md.contains("**Speaker 6** [00:00:00]\nhi"));
    }

    #[test]
    fn write_notes_creates_ari_note_md() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_notes(dir, "# Notes\n- point").unwrap();
        let body = std::fs::read_to_string(dir.join("ari-note.md")).unwrap();
        assert_eq!(body, "# Notes\n- point");
    }

    #[test]
    fn pending_audio_write_and_discard_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = write_pending_audio(root, &pmeta("2026-06-12T10:00:00.000Z"), b"mp3bytes").unwrap();
        assert_eq!(id, "2026-06-12T10-00-00.000Z");
        let audio = pending_uploads_dir(root).join("2026-06-12T10-00-00.000Z.mp3");
        let sidecar = pending_uploads_dir(root).join("2026-06-12T10-00-00.000Z.json");
        assert_eq!(std::fs::read(&audio).unwrap(), b"mp3bytes");
        assert!(sidecar.is_file());

        discard_pending_audio(root, "2026-06-12T10:00:00.000Z").unwrap();
        assert!(!audio.exists());
        assert!(!sidecar.exists());
    }

    #[test]
    fn pending_audio_distinct_subsecond_keys_do_not_collide() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Two distinct recordings within the same wall-clock second must not
        // overwrite each other.
        write_pending_audio(root, &pmeta("2026-06-12T10:00:00.123Z"), b"first").unwrap();
        write_pending_audio(root, &pmeta("2026-06-12T10:00:00.456Z"), b"second").unwrap();
        let a = pending_uploads_dir(root).join("2026-06-12T10-00-00.123Z.mp3");
        let b = pending_uploads_dir(root).join("2026-06-12T10-00-00.456Z.mp3");
        assert_eq!(std::fs::read(&a).unwrap(), b"first");
        assert_eq!(std::fs::read(&b).unwrap(), b"second");
    }

    #[test]
    fn pending_audio_overwrite_is_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_pending_audio(root, &pmeta("2026-06-12T10:00:00Z"), b"first").unwrap();
        write_pending_audio(root, &pmeta("2026-06-12T10:00:00Z"), b"second").unwrap();
        let path = pending_uploads_dir(root).join("2026-06-12T10-00-00Z.mp3");
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn discard_pending_audio_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        // No pending-uploads dir, no file — still Ok.
        discard_pending_audio(tmp.path(), "2026-06-12T10:00:00.000Z").unwrap();
    }

    #[test]
    fn pending_audio_rejects_traversal_ids() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(write_pending_audio(tmp.path(), &pmeta("a/b"), b"x").is_err());
        assert!(write_pending_audio(tmp.path(), &pmeta("\\evil"), b"x").is_err());
        assert!(discard_pending_audio(tmp.path(), "a/b").is_err());
    }

    #[test]
    fn lists_pending_uploads_paired_sorted_and_skips_orphans() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Two well-formed pending uploads (written out of order).
        write_pending_audio(root, &pmeta("2026-06-12T11:00:00Z"), b"b").unwrap();
        write_pending_audio(root, &pmeta("2026-06-12T09:00:00Z"), b"a").unwrap();
        // Orphan mp3 with no sidecar — must be skipped.
        std::fs::write(pending_uploads_dir(root).join("2026-06-12T08-00-00Z.mp3"), b"x").unwrap();
        // Orphan sidecar with no mp3 — must be skipped.
        std::fs::write(pending_uploads_dir(root).join("2026-06-12T07-00-00Z.json"), b"{}").unwrap();

        let list = list_pending_uploads(root).unwrap();
        assert_eq!(list.len(), 2);
        // Chronological ascending.
        assert_eq!(list[0].created_at, "2026-06-12T09:00:00Z");
        assert_eq!(list[1].created_at, "2026-06-12T11:00:00Z");
    }

    #[test]
    fn lists_pending_uploads_empty_when_no_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list_pending_uploads(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn combines_pending_audio_in_key_order() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_pending_audio(root, &pmeta("2026-06-12T09:00:00Z"), b"AAA").unwrap();
        write_pending_audio(root, &pmeta("2026-06-12T11:00:00Z"), b"BB").unwrap();

        let keys = vec![
            "2026-06-12T09:00:00Z".to_string(),
            "2026-06-12T11:00:00Z".to_string(),
        ];
        let combined = combine_pending_audio(root, &keys, 1024).unwrap();
        assert_eq!(combined, b"AAABB");
    }

    #[test]
    fn combine_pending_audio_errors_on_missing_key() {
        let tmp = tempfile::tempdir().unwrap();
        let keys = vec!["2026-06-12T09:00:00Z".to_string()];
        assert!(combine_pending_audio(tmp.path(), &keys, 1024).is_err());
    }

    #[test]
    fn combine_pending_audio_rejects_over_max() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_pending_audio(root, &pmeta("2026-06-12T09:00:00Z"), b"toolong").unwrap();
        let keys = vec!["2026-06-12T09:00:00Z".to_string()];
        assert!(combine_pending_audio(root, &keys, 3).is_err());
    }

    #[test]
    fn create_dir_and_roundtrip_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let id = "2026-06-02T14-30-05Z";
        let dir = create_recording_dir(root, id).unwrap();
        assert!(dir.is_dir());
        assert_eq!(dir, recordings_dir(root).join(id));

        let meta = RecordingMeta {
            id: id.to_string(),
            title: "Recording 2026-06-02 14:30".to_string(),
            created_at: "2026-06-02T14:30:05Z".to_string(),
            duration_seconds: 42,
            status: RecordingStatus::Transcribing,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
        };
        write_meta(&dir, &meta).unwrap();
        let read = read_meta(&dir).unwrap();
        assert_eq!(read, meta);
    }
}
