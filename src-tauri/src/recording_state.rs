//! Shared "is a recording in progress" flag, set when a recorder window opens
//! (manual via tray or auto via the mic monitor) and cleared when it stops or
//! is destroyed. The mic monitor reads it to suppress auto-triggers — and to
//! avoid self-triggering off a manual recording — regardless of which PID
//! macOS attributes our own capture to.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingSource {
    Manual,
    Auto,
}

/// A recorder-window open request deferred behind the yield handshake: the
/// previous pill still held the one window slot, so this waits for it to be
/// destroyed. See `commands::open_waveform_window`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOpen {
    pub meeting_id: Option<i64>,
    pub local_append_id: Option<String>,
    pub force_new: bool,
    pub auto: bool,
    /// Identifies this request so a timed-out yield only drops its own entry,
    /// never a newer one queued in the meantime.
    pub token: u64,
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
    /// A recording requested while the window slot was still held, waiting for
    /// the incumbent pill to stand down. At most one: a newer request replaces
    /// an older one rather than queueing two recordings.
    pending_open: Mutex<Option<PendingOpen>>,
    next_open_token: AtomicU64,
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

    /// Queue a recorder-window open to run once the incumbent pill is
    /// destroyed. Replaces any earlier queued request (newest intent wins) and
    /// returns the token identifying this one.
    pub fn queue_reopen(
        &self,
        meeting_id: Option<i64>,
        local_append_id: Option<String>,
        force_new: bool,
        auto: bool,
    ) -> u64 {
        let mut pending_open = self.pending_open.lock().unwrap();
        let token = self.next_open_token.fetch_add(1, Ordering::Relaxed);
        *pending_open = Some(PendingOpen {
            meeting_id,
            local_append_id,
            force_new,
            auto,
            token,
        });
        token
    }

    /// Claim the queued request, if any, leaving the slot empty.
    pub fn take_reopen(&self) -> Option<PendingOpen> {
        self.pending_open.lock().unwrap().take()
    }

    /// Drop a queued request the pill never honored (it refused to yield
    /// because it was still capturing or uploading). No-op once the request has
    /// been taken, or once a newer one has replaced it.
    pub fn expire_reopen(&self, token: u64) -> bool {
        let mut slot = self.pending_open.lock().unwrap();
        if slot.as_ref().map(|p| p.token) == Some(token) {
            *slot = None;
            return true;
        }
        false
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
    fn queued_reopen_round_trips_and_empties_the_slot() {
        let s = RecordingState::new();
        assert_eq!(s.take_reopen(), None);

        let token = s.queue_reopen(Some(42), Some("rec-1".into()), true, false);
        let queued = s.take_reopen().expect("a request was queued");
        assert_eq!(queued.meeting_id, Some(42));
        assert_eq!(queued.local_append_id.as_deref(), Some("rec-1"));
        assert!(queued.force_new);
        assert!(!queued.auto);
        assert_eq!(queued.token, token);

        // Taking is destructive: the destroyed pill must not re-open twice.
        assert_eq!(s.take_reopen(), None);
    }

    #[test]
    fn expiring_a_reopen_only_drops_its_own_request() {
        let s = RecordingState::new();
        let stale = s.queue_reopen(None, None, false, false);

        // A second request supersedes the first, so the first's timeout must
        // not cancel the recording the user just asked for.
        let fresh = s.queue_reopen(Some(7), None, false, true);
        assert!(!s.expire_reopen(stale));
        assert_eq!(s.take_reopen().map(|p| p.meeting_id), Some(Some(7)));

        // Nothing left to expire once the request has been honored.
        assert!(!s.expire_reopen(fresh));
    }

    #[test]
    fn expiring_a_reopen_clears_a_refused_yield() {
        let s = RecordingState::new();
        let token = s.queue_reopen(Some(1), None, false, false);

        // The pill refused to stand down (still capturing or uploading), so the
        // request must not survive to hijack an unrelated later close.
        assert!(s.expire_reopen(token));
        assert_eq!(s.take_reopen(), None);
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

    #[test]
    fn concurrent_queue_reopen_leaves_the_highest_token_queued() {
        use std::sync::{Arc, Barrier};

        const CONTENDERS: u64 = 16;
        let state = Arc::new(RecordingState::new());
        let barrier = Arc::new(Barrier::new(CONTENDERS as usize));
        let handles: Vec<_> = (0..CONTENDERS)
            .map(|_| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.queue_reopen(None, None, false, false)
                })
            })
            .collect();

        let max_token = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .max()
            .unwrap();

        // Token allocation and slot replacement happen under the same lock, so
        // whichever request acquired the lock last also holds the highest
        // token — an earlier allocation can never overwrite a later one.
        let queued = state.take_reopen().expect("a request was queued");
        assert_eq!(queued.token, max_token);
    }
}
