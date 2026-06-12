//! Next-meeting tray orchestrator (native).
//!
//! Fetches today's scheduled meetings from the Ariso API, features the next
//! strictly-upcoming one in the menu-bar tray (countdown title + menu rows),
//! and promotes the following meeting once it starts. Lives in the Rust
//! process for the same reason as `meeting_notifications`: macOS suspends
//! hidden webviews, so a webview timer would freeze in the background.

use std::sync::Mutex;

use chrono::{DateTime, Utc};

/// A meeting from `GET /meetings` (list payload carries no end time).
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledMeeting {
    pub id: i64,
    pub title: Option<String>,
    pub start_at: DateTime<Utc>,
}

/// The meeting currently surfaced in the tray. `end_at` is resolved
/// separately via `GET /meeting-notes/:id` and may be absent.
#[derive(Clone, Debug, PartialEq)]
pub struct FeaturedMeeting {
    pub id: i64,
    pub title: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: Option<DateTime<Utc>>,
}

/// Holds the handle to the running orchestrator task (if any).
#[derive(Default)]
pub struct TrayMeetingManager {
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl TrayMeetingManager {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The currently featured meeting, readable by the tray menu builder and the
/// `record_featured` menu-event arm (both need it from `'static` contexts).
#[derive(Default)]
pub struct FeaturedMeetingState(pub Mutex<Option<FeaturedMeeting>>);

impl FeaturedMeetingState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Max characters of the meeting title shown in the menu-bar (tray) title.
const TITLE_MAX_CHARS: usize = 10;

/// Truncate a meeting title for the tray title bar. `None`/blank titles
/// become "Untitled meeting"; titles longer than 10 Unicode scalar values
/// are cut to 10 + `…`.
pub(crate) fn truncate_title(title: Option<&str>) -> String {
    let Some(s) = title.filter(|s| !s.trim().is_empty()) else {
        return "Untitled meeting".to_string();
    };
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= TITLE_MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = chars[..TITLE_MAX_CHARS].iter().collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_title_none_is_untitled() {
        assert_eq!(truncate_title(None), "Untitled meeting");
    }

    #[test]
    fn truncate_title_blank_is_untitled() {
        assert_eq!(truncate_title(Some("")), "Untitled meeting");
        assert_eq!(truncate_title(Some("   ")), "Untitled meeting");
    }

    #[test]
    fn truncate_title_short_unchanged() {
        assert_eq!(truncate_title(Some("Standup")), "Standup");
        assert_eq!(truncate_title(Some("Exactly10!")), "Exactly10!");
    }

    #[test]
    fn truncate_title_long_truncated_to_10_plus_ellipsis() {
        assert_eq!(truncate_title(Some("Weekly Engineering Sync")), "Weekly Eng…");
    }

    #[test]
    fn truncate_title_counts_unicode_scalars_not_bytes() {
        assert_eq!(truncate_title(Some("héllo wörld plus")), "héllo wörl…");
    }
}
