//! Auto-trigger microphone monitor. Watches per-process audio input via
//! CoreAudio (macOS 14.4+), excludes our own PID, and runs a debounced state
//! machine. On a sustained external mic-on it opens the recorder window; on a
//! sustained mic-off it emits `auto-record://stop`.

use std::collections::HashSet;

/// Time (ms) an external app must hold the mic before we start recording.
const START_DEBOUNCE_MS: u64 = 3_000;
/// Time (ms) the triggering apps must all stay released before we stop.
const STOP_DEBOUNCE_MS: u64 = 8_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
}

#[derive(Debug, Default)]
enum Phase {
    #[default]
    Idle,
    Arming {
        since: u64,
    },
    Recording {
        trigger: HashSet<i32>,
    },
    Stopping {
        trigger: HashSet<i32>,
        since: u64,
    },
}

#[derive(Default)]
pub struct Machine {
    phase: Phase,
}

impl Machine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance one tick. `now` is a monotonic millisecond counter. `external`
    /// is the set of PIDs (excluding our own) currently running audio input.
    /// `recording_active` is whether any recording (manual or auto) is in
    /// progress. Returns an `Action` when a transition demands one.
    pub fn tick(
        &mut self,
        now: u64,
        external: &HashSet<i32>,
        recording_active: bool,
    ) -> Option<Action> {
        match &self.phase {
            Phase::Idle => {
                if !external.is_empty() && !recording_active {
                    self.phase = Phase::Arming { since: now };
                }
                None
            }
            Phase::Arming { since } => {
                if external.is_empty() {
                    self.phase = Phase::Idle;
                    None
                } else if now.saturating_sub(*since) >= START_DEBOUNCE_MS {
                    if recording_active {
                        self.phase = Phase::Idle;
                        None
                    } else {
                        // Snapshot the triggering PIDs; only these decide when
                        // the meeting is over, so our own later capture (a new
                        // PID) never keeps it artificially alive.
                        self.phase = Phase::Recording {
                            trigger: external.clone(),
                        };
                        Some(Action::Start)
                    }
                } else {
                    None
                }
            }
            Phase::Recording { trigger } => {
                let alive = trigger.iter().any(|p| external.contains(p));
                if !alive {
                    self.phase = Phase::Stopping {
                        trigger: trigger.clone(),
                        since: now,
                    };
                }
                None
            }
            Phase::Stopping { trigger, since } => {
                let alive = trigger.iter().any(|p| external.contains(p));
                if alive {
                    self.phase = Phase::Recording {
                        trigger: trigger.clone(),
                    };
                    None
                } else if now.saturating_sub(*since) >= STOP_DEBOUNCE_MS {
                    self.phase = Phase::Idle;
                    Some(Action::Stop)
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pids(list: &[i32]) -> HashSet<i32> {
        list.iter().copied().collect()
    }

    #[test]
    fn brief_blip_does_not_start() {
        let mut m = Machine::new();
        assert_eq!(m.tick(0, &pids(&[42]), false), None); // -> Arming
        // Mic released before the 3s debounce elapses.
        assert_eq!(m.tick(1_000, &pids(&[]), false), None); // -> Idle
        assert_eq!(m.tick(5_000, &pids(&[]), false), None);
    }

    #[test]
    fn sustained_mic_starts_after_debounce() {
        let mut m = Machine::new();
        assert_eq!(m.tick(0, &pids(&[42]), false), None); // Arming
        assert_eq!(m.tick(2_000, &pids(&[42]), false), None); // still arming
        assert_eq!(m.tick(3_000, &pids(&[42]), false), Some(Action::Start));
    }

    #[test]
    fn active_recording_suppresses_start() {
        let mut m = Machine::new();
        // recording_active true keeps us out of Arming entirely.
        assert_eq!(m.tick(0, &pids(&[42]), true), None);
        assert_eq!(m.tick(3_000, &pids(&[42]), true), None);
    }

    #[test]
    fn stops_after_trigger_pids_release() {
        let mut m = Machine::new();
        m.tick(0, &pids(&[42]), false);
        assert_eq!(m.tick(3_000, &pids(&[42]), false), Some(Action::Start));
        // Our own capture appears as a new PID — must be ignored.
        assert_eq!(m.tick(4_000, &pids(&[42, 99]), false), None);
        // Trigger app releases; only PID 99 (ours) remains.
        assert_eq!(m.tick(5_000, &pids(&[99]), false), None); // -> Stopping
        assert_eq!(m.tick(13_000, &pids(&[99]), false), Some(Action::Stop));
    }

    #[test]
    fn reacquire_during_stopping_cancels_stop() {
        let mut m = Machine::new();
        m.tick(0, &pids(&[42]), false);
        m.tick(3_000, &pids(&[42]), false); // Start
        m.tick(4_000, &pids(&[]), false); // Stopping
        // Trigger app comes back before the 8s stop debounce.
        assert_eq!(m.tick(6_000, &pids(&[42]), false), None); // -> Recording
        assert_eq!(m.tick(20_000, &pids(&[42]), false), None); // stays recording
    }
}
