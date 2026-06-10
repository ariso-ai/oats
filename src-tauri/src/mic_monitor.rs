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

/// PIDs (excluding our own) currently running audio input, via CoreAudio.
/// Returns an empty set off macOS or when the API is unavailable.
fn external_input_pids() -> HashSet<i32> {
    #[cfg(target_os = "macos")]
    {
        coreaudio::external_input_pids(std::process::id() as i32)
    }
    #[cfg(not(target_os = "macos"))]
    {
        HashSet::new()
    }
}

/// Whether the per-process input API (macOS 14.4+) is available at runtime.
pub fn is_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        coreaudio::is_supported()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
mod coreaudio {
    use std::collections::HashSet;
    use std::ffi::c_void;

    type OSStatus = i32;
    type AudioObjectID = u32;

    const SYSTEM_OBJECT: AudioObjectID = 1; // kAudioObjectSystemObject
    const SCOPE_GLOBAL: u32 = fourcc(b"glob"); // kAudioObjectPropertyScopeGlobal
    const ELEMENT_MAIN: u32 = 0; // kAudioObjectPropertyElementMain
    const PROCESS_OBJECT_LIST: u32 = fourcc(b"prs#"); // kAudioHardwarePropertyProcessObjectList
    const PROCESS_PID: u32 = fourcc(b"ppid"); // kAudioProcessPropertyPID
    const IS_RUNNING_INPUT: u32 = fourcc(b"piri"); // kAudioProcessPropertyIsRunningInput (14.4+)

    const fn fourcc(s: &[u8; 4]) -> u32 {
        ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
    }

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            out_size: *mut u32,
        ) -> OSStatus;

        fn AudioObjectGetPropertyData(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            io_size: *mut u32,
            out_data: *mut c_void,
        ) -> OSStatus;
    }

    fn global_address(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            selector,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        }
    }

    fn process_object_list() -> Option<Vec<AudioObjectID>> {
        let addr = global_address(PROCESS_OBJECT_LIST);
        let mut size: u32 = 0;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size)
        };
        if status != 0 {
            return None;
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids: Vec<AudioObjectID> = vec![0; count];
        if count == 0 {
            return Some(ids);
        }
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                ids.as_mut_ptr() as *mut c_void,
            )
        };
        if status != 0 {
            return None;
        }
        Some(ids)
    }

    fn read_u32(object: AudioObjectID, selector: u32) -> Option<u32> {
        let addr = global_address(selector);
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let mut value: u32 = 0;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut value as *mut u32 as *mut c_void,
            )
        };
        if status != 0 {
            None
        } else {
            Some(value)
        }
    }

    pub fn external_input_pids(our_pid: i32) -> HashSet<i32> {
        let mut set = HashSet::new();
        let Some(objects) = process_object_list() else {
            return set;
        };
        for obj in objects {
            if read_u32(obj, IS_RUNNING_INPUT).unwrap_or(0) == 0 {
                continue;
            }
            if let Some(pid) = read_u32(obj, PROCESS_PID).map(|v| v as i32) {
                if pid > 0 && pid != our_pid {
                    set.insert(pid);
                }
            }
        }
        set
    }

    /// Probe availability: the process-object-list selector is 14.0+, and the
    /// IsRunningInput property is 14.4+. If either read fails the API is
    /// unavailable on this OS and the monitor must stay off.
    pub fn is_supported() -> bool {
        let Some(objects) = process_object_list() else {
            return false;
        };
        match objects.first() {
            Some(&obj) => read_u32(obj, IS_RUNNING_INPUT).is_some(),
            // No audio processes to probe right now — the list call succeeded,
            // which already requires 14.0+; treat as supported.
            None => true,
        }
    }
}
