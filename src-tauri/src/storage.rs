use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub status: RecordingStatus,
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

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", meta.title));
    out.push_str(&format!("date: {}\n", meta.created_at));
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
            Ok(m) => out.push(RecordingSummary {
                id: m.id,
                title: m.title,
                created_at: m.created_at,
                duration_seconds: m.duration_seconds,
                status: m.status,
            }),
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
            participants: vec![], model_version: None, error: None,
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
        };
        let segments = vec![
            Segment { speaker: 0, text: "Hello there".into(), start: 3.0, end: 9.0 },
            Segment { speaker: 1, text: "Hi back".into(), start: 9.0, end: 12.0 },
        ];
        let md = render_markdown(&meta, &segments);

        assert!(md.starts_with("---\n"));
        assert!(md.contains("title: Recording 2026-06-02 14:30"));
        assert!(md.contains("duration: \"00:42:13\""));
        assert!(md.contains("**Speaker 1** [00:00:03]\nHello there"));
        assert!(md.contains("**Speaker 2** [00:00:09]\nHi back"));
    }

    #[test]
    fn unknown_speaker_falls_back_to_label() {
        let meta = RecordingMeta {
            id: "x".into(), title: "t".into(), created_at: "c".into(),
            duration_seconds: 0, status: RecordingStatus::Done, language: None,
            participants: vec![], model_version: None, error: None,
        };
        let segments = vec![Segment { speaker: 5, text: "hi".into(), start: 0.0, end: 1.0 }];
        let md = render_markdown(&meta, &segments);
        assert!(md.contains("**Speaker 6** [00:00:00]\nhi"));
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
        };
        write_meta(&dir, &meta).unwrap();
        let read = read_meta(&dir).unwrap();
        assert_eq!(read, meta);
    }
}
