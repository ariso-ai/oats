use serde::{Deserialize, Serialize};
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

/// Turn an ISO-8601 instant into a filesystem-safe, sortable folder id.
/// "2026-06-02T14:30:05.123Z" -> "2026-06-02T14-30-05Z"
pub fn sanitize_iso_to_id(iso: &str) -> String {
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
    fn formats_hms() {
        assert_eq!(format_hms(0.0), "00:00:00");
        assert_eq!(format_hms(3.4), "00:00:03");
        assert_eq!(format_hms(2533.0), "00:42:13");
    }
}
