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

/// Everything that describes the recording currently in progress. These fields
/// live behind ONE mutex on purpose: they are read and written as compound
/// operations ("if a recording is active, record its identity", "snapshot what
/// the tray menu should show"), and splitting them across separate locks let a
/// concurrent `clear()` interleave — leaving an inactive state holding a stale
/// identity, or a reader seeing `source: None` alongside a live `meeting_id`.
#[derive(Default)]
struct Lifecycle {
    source: Option<RecordingSource>,
    /// Meeting id the current recording is attached to (if any). Late-joining
    /// windows (e.g. a library window opened mid-recording) read this so they
    /// can re-select the attached meeting without relying on the one-shot
    /// `recording://started` event.
    meeting_id: Option<i64>,
    /// Human-readable name of what is being recorded, for the tray's recording
    /// menu. Set by the recorder window once the meeting (or the local
    /// recording's default label) is known, so it lags `meeting_id` by however
    /// long resolution takes. `None` until then, and for the whole session if
    /// resolution fails.
    title: Option<String>,
    /// Last pause state pushed by `set_tray_recording`. Cached so a menu
    /// rebuild triggered by anything *other* than pause/resume (a late-arriving
    /// title, say) does not flip a paused recording's menu back to "Pause".
    paused: bool,
}

/// A consistent view of the active recording, taken under the lifecycle lock.
/// The tray builds its recording menu from this so the menu it commits can
/// never mix fields from before and after a lifecycle transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingSnapshot {
    pub is_paused: bool,
    pub title: Option<String>,
    pub meeting_id: Option<i64>,
}

#[derive(Default)]
pub struct RecordingState {
    lifecycle: Mutex<Lifecycle>,
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

    /// Recover a poisoned lifecycle lock rather than crashing the tray: a
    /// previous panic while holding it must not take the whole menu bar down.
    fn lifecycle(&self) -> std::sync::MutexGuard<'_, Lifecycle> {
        self.lifecycle.lock().unwrap_or_else(|poisoned| {
            eprintln!("recording_state: lifecycle mutex poisoned; recovering");
            poisoned.into_inner()
        })
    }

    pub fn set(&self, source: RecordingSource, meeting_id: Option<i64>) {
        *self.lifecycle() = Lifecycle {
            source: Some(source),
            meeting_id,
            title: None,
            paused: false,
        };
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        *self.lifecycle() = Lifecycle::default();
        self.capture.store(false, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.lifecycle().source.is_some()
    }

    /// The meeting the *active* recording is attached to. Returns `None` once
    /// the recording ends, so a caller can never act on an id that outlived it.
    pub fn active_meeting_id(&self) -> Option<i64> {
        let g = self.lifecycle();
        g.source.as_ref()?;
        g.meeting_id
    }

    /// Record what the active recording is attached to, once the recorder
    /// window knows. Local recordings pass `meeting_id: None` with a title —
    /// they have no server-side meeting, and the tray routes their click
    /// through the same `recording://reveal` broadcast either way.
    ///
    /// The active check and both writes happen under one lock, so a `clear()`
    /// racing the identity push either wins outright (this returns `false` and
    /// writes nothing) or loses outright — it can never interleave and leave an
    /// ended recording holding a stale identity. Returns whether the identity
    /// was recorded.
    pub fn set_recording_meeting(&self, meeting_id: Option<i64>, title: Option<String>) -> bool {
        let mut g = self.lifecycle();
        if g.source.is_none() {
            return false;
        }
        g.meeting_id = meeting_id;
        g.title = title.filter(|t| !t.trim().is_empty());
        true
    }

    pub fn active_recording_title(&self) -> Option<String> {
        let g = self.lifecycle();
        g.source.as_ref()?;
        g.title.clone()
    }

    /// A consistent view of the active recording, or `None` when nothing is
    /// recording. Callers that commit a tray menu must take this while holding
    /// the menu-commit lock (see `tray::refresh_recording_menu`) so the menu
    /// they build cannot land after a later transition's menu.
    pub fn active_snapshot(&self) -> Option<RecordingSnapshot> {
        let g = self.lifecycle();
        g.source.as_ref()?;
        Some(RecordingSnapshot {
            is_paused: g.paused,
            title: g.title.clone(),
            meeting_id: g.meeting_id,
        })
    }

    /// No-op when nothing is recording, so a late pause/resume cannot leave a
    /// pause flag behind for the next recording to inherit.
    pub fn set_paused(&self, paused: bool) {
        let mut g = self.lifecycle();
        if g.source.is_some() {
            g.paused = paused;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.lifecycle().paused
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
    fn recording_meeting_and_title_round_trip_and_clear() {
        let s = RecordingState::new();
        assert_eq!(s.active_meeting_id(), None);
        assert_eq!(s.active_recording_title(), None);

        s.set(RecordingSource::Auto, None);
        // An auto-triggered recording that matched no calendar meeting learns
        // its identity later, once the ad-hoc meeting exists.
        s.set_recording_meeting(Some(42), Some("Budget sync".into()));
        assert_eq!(s.active_meeting_id(), Some(42));
        assert_eq!(s.active_recording_title().as_deref(), Some("Budget sync"));

        // A local recording has a title but no server-side meeting to route to.
        s.set_recording_meeting(None, Some("Tue Jun 2 @ 2:30PM".into()));
        assert_eq!(s.active_meeting_id(), None);
        assert_eq!(s.active_recording_title().as_deref(), Some("Tue Jun 2 @ 2:30PM"));

        s.clear();
        assert_eq!(s.active_meeting_id(), None);
        assert_eq!(s.active_recording_title(), None);
    }

    /// The identity push and the stop path are independent async commands, so
    /// a push can arrive after the recording ended (finalize resolves a meeting
    /// id for an unattached Ariso upload). It must be rejected, not applied.
    #[test]
    fn an_identity_push_is_rejected_once_the_recording_ended() {
        let s = RecordingState::new();
        s.set(RecordingSource::Auto, None);
        assert!(s.set_recording_meeting(Some(42), Some("Budget sync".into())));

        s.clear();
        assert!(!s.set_recording_meeting(Some(725), Some("Late arrival".into())));
        assert_eq!(s.active_meeting_id(), None);
        assert_eq!(s.active_recording_title(), None);
        assert_eq!(s.active_snapshot(), None);
    }

    /// Regression guard for the split-lock bug: `clear()` racing an identity
    /// push must never leave an ended recording still holding an identity.
    /// With `meeting_id` and `title` behind separate mutexes, a clear landing
    /// between the two writes left `title` populated on an inactive state.
    #[test]
    fn clear_racing_an_identity_push_never_leaves_a_stale_identity() {
        use std::sync::{Arc, Barrier};

        for _ in 0..500 {
            let state = Arc::new(RecordingState::new());
            state.set(RecordingSource::Auto, None);
            let barrier = Arc::new(Barrier::new(2));

            let pusher = {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.set_recording_meeting(Some(725), Some("Untitled meeting".into()))
                })
            };
            let clearer = {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.clear();
                })
            };
            let pushed = pusher.join().unwrap();
            clearer.join().unwrap();

            // The clear always lands last here (the push either beat it and was
            // wiped, or lost and wrote nothing), so the end state is inactive
            // with no identity — whichever way the race resolved.
            assert!(!state.is_active());
            assert_eq!(state.active_meeting_id(), None, "pushed={pushed}");
            assert_eq!(state.active_recording_title(), None, "pushed={pushed}");
            assert_eq!(state.active_snapshot(), None, "pushed={pushed}");
        }
    }

    /// The tray builds its recording menu from one snapshot. A snapshot taken
    /// while `clear()` runs must be all-or-nothing: either the full identity of
    /// the recording that was still active, or `None` — never a half-cleared
    /// mix (e.g. a live title beside an already-wiped meeting id).
    #[test]
    fn a_snapshot_never_mixes_fields_from_across_a_clear() {
        use std::sync::{Arc, Barrier};

        for _ in 0..500 {
            let state = Arc::new(RecordingState::new());
            state.set(RecordingSource::Manual, Some(725));
            state.set_recording_meeting(Some(725), Some("Budget sync".into()));
            let barrier = Arc::new(Barrier::new(2));

            let reader = {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.active_snapshot()
                })
            };
            let clearer = {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    state.clear();
                })
            };
            let snapshot = reader.join().unwrap();
            clearer.join().unwrap();

            if let Some(snapshot) = snapshot {
                assert_eq!(snapshot.meeting_id, Some(725));
                assert_eq!(snapshot.title.as_deref(), Some("Budget sync"));
            }
        }
    }

    /// A pause arriving after the recording ended must not leave a flag behind
    /// for the next recording's menu to inherit.
    #[test]
    fn pausing_an_ended_recording_is_a_no_op() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual, None);
        s.clear();

        s.set_paused(true);
        assert!(!s.is_paused());
    }

    /// A new recording must never inherit the previous one's title.
    #[test]
    fn starting_a_recording_drops_the_previous_title() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual, Some(1));
        s.set_recording_meeting(Some(1), Some("Standup".into()));

        s.set(RecordingSource::Auto, None);
        assert_eq!(s.active_recording_title(), None);
        assert_eq!(s.active_meeting_id(), None);
    }

    #[test]
    fn paused_flag_round_trips_and_resets_per_recording() {
        let s = RecordingState::new();
        assert!(!s.is_paused());

        s.set(RecordingSource::Manual, None);
        s.set_paused(true);
        assert!(s.is_paused());
        s.set_paused(false);
        assert!(!s.is_paused());

        s.set_paused(true);
        // A fresh recording always starts running.
        s.set(RecordingSource::Auto, None);
        assert!(!s.is_paused());

        s.set_paused(true);
        s.clear();
        assert!(!s.is_paused());
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
