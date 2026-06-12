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

/// Relative countdown to a future start time: `in 12min`, or `in <1min`
/// under a minute. Callers only pass strictly-upcoming meetings; a stale
/// (already-started) one reads `in <1min` until the next tick drops it.
pub(crate) fn format_countdown(start: DateTime<Utc>, now: DateTime<Utc>) -> String {
    // num_minutes truncates toward zero == floor for positive durations.
    let mins = (start - now).num_minutes();
    if mins >= 1 {
        format!("in {mins}min")
    } else {
        "in <1min".to_string()
    }
}

/// The full menu-bar string next to the tray icon, e.g. `Weekly Eng… in 12min`.
pub(crate) fn format_title_bar(
    title: Option<&str>,
    start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    format!("{} {}", truncate_title(title), format_countdown(start, now))
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

    /// Parse an RFC 3339 timestamp into UTC for test fixtures.
    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn countdown_whole_minutes() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:12:00Z"), t("2026-06-11T10:00:00Z")),
            "in 12min"
        );
    }

    #[test]
    fn countdown_floors_partial_minutes() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:12:30Z"), t("2026-06-11T10:00:00Z")),
            "in 12min"
        );
    }

    #[test]
    fn countdown_exactly_one_minute() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:01:00Z"), t("2026-06-11T10:00:00Z")),
            "in 1min"
        );
    }

    #[test]
    fn countdown_under_a_minute() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:00:59Z"), t("2026-06-11T10:00:00Z")),
            "in <1min"
        );
    }

    #[test]
    fn title_bar_composes_truncated_title_and_countdown() {
        assert_eq!(
            format_title_bar(
                Some("Weekly Engineering Sync"),
                t("2026-06-11T10:12:00Z"),
                t("2026-06-11T10:00:00Z")
            ),
            "Weekly Eng… in 12min"
        );
    }

    #[test]
    fn title_bar_untitled_meeting() {
        assert_eq!(
            format_title_bar(None, t("2026-06-11T10:00:30Z"), t("2026-06-11T10:00:00Z")),
            "Untitled meeting in <1min"
        );
    }
}
