//! Shared "is a recording in progress" flag, set when a recorder window opens
//! (manual via tray or auto via the mic monitor) and cleared when it stops or
//! is destroyed. The mic monitor reads it to suppress auto-triggers — and to
//! avoid self-triggering off a manual recording — regardless of which PID
//! macOS attributes our own capture to.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingSource {
    Manual,
    Auto,
}

#[derive(Default)]
pub struct RecordingState {
    inner: Mutex<Option<RecordingSource>>,
    /// Meeting id the current recording is attached to (if any). Late-joining
    /// windows (e.g. a library window opened mid-recording) read this so they
    /// can re-select the attached meeting without relying on the one-shot
    /// `recording://started` event.
    meeting_id: Mutex<Option<i64>>,
    /// Whether audio capture has actually started (getUserMedia resolved) —
    /// set via `set_tray_recording` from the recorder window. The pill
    /// visibility watcher must not hide the window before this point.
    capture: AtomicBool,
}

impl RecordingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, source: RecordingSource, meeting_id: Option<i64>) {
        *self.inner.lock().unwrap() = Some(source);
        *self.meeting_id.lock().unwrap() = meeting_id;
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = None;
        *self.meeting_id.lock().unwrap() = None;
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    pub fn active_meeting_id(&self) -> Option<i64> {
        *self.meeting_id.lock().unwrap()
    }

    pub fn mark_capture_active(&self) {
        self.capture.store(true, Ordering::Relaxed);
    }

    pub fn capture_active(&self) -> bool {
        self.capture.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_inactive() {
        assert!(!RecordingState::new().is_active());
    }

    #[test]
    fn set_marks_active_clear_resets() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual, None);
        assert!(s.is_active());
        s.set(RecordingSource::Auto, None);
        assert!(s.is_active());
        s.clear();
        assert!(!s.is_active());
    }

    #[test]
    fn capture_flag_starts_false_and_resets_per_recording() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual, None);
        assert!(!s.capture_active());
        s.mark_capture_active();
        assert!(s.capture_active());
        // A new recording starts with capture not yet running.
        s.set(RecordingSource::Auto, None);
        assert!(!s.capture_active());
        s.mark_capture_active();
        s.clear();
        assert!(!s.capture_active());
    }

    #[test]
    fn meeting_id_round_trips_and_clears() {
        let s = RecordingState::new();
        assert_eq!(s.active_meeting_id(), None);
        s.set(RecordingSource::Manual, Some(42));
        assert_eq!(s.active_meeting_id(), Some(42));
        // A new recording without a meeting drops the previous id.
        s.set(RecordingSource::Auto, None);
        assert_eq!(s.active_meeting_id(), None);
        s.set(RecordingSource::Manual, Some(7));
        s.clear();
        assert_eq!(s.active_meeting_id(), None);
    }
}
