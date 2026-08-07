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
    /// Process-wide ownership of the recorder pill window. Acquired before
    /// native window construction and released only after destruction.
    window_claimed: AtomicBool,
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

    /// Claim the one recorder-pill window slot. Unlike recording-active state,
    /// this remains held while the window is uploading or closing.
    pub fn try_claim_window(&self) -> bool {
        self.window_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn release_window_claim(&self) {
        self.window_claimed.store(false, Ordering::Release);
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

    #[test]
    fn recorder_window_claim_is_exclusive_until_destroyed() {
        let s = RecordingState::new();
        assert!(s.try_claim_window());
        assert!(!s.try_claim_window());

        // Stopping capture does not release the window: it may still own an
        // upload, retry, or native close transition.
        s.clear();
        assert!(!s.try_claim_window());

        s.release_window_claim();
        assert!(s.try_claim_window());
    }

    #[test]
    fn concurrent_recorder_window_claim_has_exactly_one_winner() {
        use std::sync::{Arc, Barrier};

        const CONTENDERS: usize = 16;
        let state = Arc::new(RecordingState::new());
        let barrier = Arc::new(Barrier::new(CONTENDERS));
        let handles: Vec<_> = (0..CONTENDERS)
            .map(|_| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.try_claim_window()
                })
            })
            .collect();

        let winners = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }
}
