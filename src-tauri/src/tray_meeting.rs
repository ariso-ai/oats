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
