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
    /// Whether audio capture has actually started (getUserMedia resolved) —
    /// set via `set_tray_recording` from the recorder window. The pill
    /// visibility watcher must not hide the window before this point.
    capture: AtomicBool,
}

impl RecordingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, source: RecordingSource) {
        *self.inner.lock().unwrap() = Some(source);
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = None;
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().is_some()
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
        s.set(RecordingSource::Manual);
        assert!(s.is_active());
        s.set(RecordingSource::Auto);
        assert!(s.is_active());
        s.clear();
        assert!(!s.is_active());
    }

    #[test]
    fn capture_flag_starts_false_and_resets_per_recording() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual);
        assert!(!s.capture_active());
        s.mark_capture_active();
        assert!(s.capture_active());
        // A new recording starts with capture not yet running.
        s.set(RecordingSource::Auto);
        assert!(!s.capture_active());
        s.mark_capture_active();
        s.clear();
        assert!(!s.capture_active());
    }
}
