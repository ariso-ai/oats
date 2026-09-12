//! The floating recorder pill: when it is visible, and the bridge between the
//! natively drawn pill (Swift on macOS, Win32 on Windows) and the recorder.
//!
//! While a recording is on-going, the recording UI normally lives in the
//! library window's embedded recorder strip; the pill is the fallback shown
//! only when that strip can't be seen — the library window is minimized or
//! closed. Tauri emits no minimize/restore events, so a watcher task polls
//! and exits once the waveform window is gone.
//!
//! The recording session itself runs in the "waveform" webview, which paints
//! nothing and is hidden once capture starts. The native pill renders its
//! `recorder://state` broadcasts and turns clicks into the events it handles.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::{AppHandle, Emitter, Listener, Manager};

#[cfg(any(target_os = "windows", test))]
mod layout;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
mod win32;
#[cfg(target_os = "windows")]
use win32 as native;

/// Other platforms (not shipped) have no pill.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod native {
    pub(super) fn create(_app: &tauri::AppHandle) {}
    pub(super) fn update(_app: &tauri::AppHandle, _state: super::PillState) {}
    pub(super) fn set_visible(_app: &tauri::AppHandle, _visible: bool) {}
    pub(super) fn destroy(_app: &tauri::AppHandle) {}
}

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// Set once the recorder broadcasts `closed`, so the watcher can't re-reveal a
/// pill whose recording is over while its window is still being torn down.
static NATIVE_CLOSED: AtomicBool = AtomicBool::new(false);

/// Bumped every time [`spawn_watcher`] starts a new watcher, so a stale
/// watcher whose "waveform" window was replaced by a queued reopen between
/// polls (same label, new window) recognizes it's no longer current and
/// exits instead of running forever alongside the new one.
static WATCHER_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The recorder's lifecycle; discriminants are the native ABI's phase codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PillPhase {
    Starting = 0,
    Recording = 1,
    Uploading = 2,
    Success = 3,
    Failed = 4,
}

/// One `recorder://state` broadcast, reduced to what the pill draws.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PillState {
    pub phase: PillPhase,
    pub paused: bool,
    pub duration_s: u32,
    /// Left-to-right levels in 0…1.
    pub bars: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StateUpdate {
    Show(PillState),
    /// The recorder is going away; hide until its window is destroyed.
    Closed,
}

/// Parse a `recorder://state` payload (see `broadcastState` in
/// WaveformView.vue). Returns `None` for anything without a known `phase`.
pub(crate) fn parse_state(payload: &str) -> Option<StateUpdate> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let phase = match value.get("phase")?.as_str()? {
        "starting" => PillPhase::Starting,
        "recording" => PillPhase::Recording,
        "uploading" => PillPhase::Uploading,
        "success" => PillPhase::Success,
        "failed" => PillPhase::Failed,
        "closed" => return Some(StateUpdate::Closed),
        _ => return None,
    };
    let mut bars = [0.0_f32; 3];
    if let Some(levels) = value.get("bars").and_then(|b| b.as_array()) {
        for (bar, level) in bars.iter_mut().zip(levels) {
            *bar = (level.as_f64().unwrap_or(0.0) as f32).clamp(0.0, 1.0);
        }
    }
    Some(StateUpdate::Show(PillState {
        phase,
        paused: value.get("isPaused").and_then(|p| p.as_bool()).unwrap_or(false),
        duration_s: value
            .get("durationSeconds")
            .and_then(|d| d.as_u64())
            .map_or(0, |d| d.min(u64::from(u32::MAX)) as u32),
        bars,
    }))
}

/// What a click on the native pill asks for; discriminants are the native
/// ABI's action codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PillAction {
    OpenMeetings = 0,
    Pause = 1,
    Resume = 2,
    Stop = 3,
    RetryUpload = 4,
    ContinueRecording = 5,
    DiscardRecording = 6,
}

impl PillAction {
    pub(crate) fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::OpenMeetings,
            1 => Self::Pause,
            2 => Self::Resume,
            3 => Self::Stop,
            4 => Self::RetryUpload,
            5 => Self::ContinueRecording,
            6 => Self::DiscardRecording,
            _ => return None,
        })
    }

    /// The event the recorder webview handles for this action: the same ones
    /// the tray menu and the Meetings strip send, plus the failed-upload
    /// controls that only the pill offers.
    pub(crate) fn event(self) -> Option<&'static str> {
        Some(match self {
            Self::OpenMeetings => return None,
            Self::Pause => "tray://pause-recording",
            Self::Resume => "tray://resume-recording",
            Self::Stop => "tray://stop-recording",
            Self::RetryUpload => "recorder://retry-upload",
            Self::ContinueRecording => "recorder://continue-recording",
            Self::DiscardRecording => "recorder://discard-recording",
        })
    }
}

/// Carry out a native pill click. Opening Meetings mirrors the webview pill:
/// surface the window, then ask it to reveal the meeting being recorded.
pub(crate) fn dispatch(app: &AppHandle, action: PillAction) {
    let Some(event) = action.event() else {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = crate::commands::create_library_window(app.clone()).await {
                eprintln!("recorder pill: failed to open Meetings: {error}");
                return;
            }
            let _ = app.emit("recording://reveal", ());
        });
        return;
    };
    let _ = app.emit(event, ());
}

/// Forward recorder broadcasts to the native pill for the app's lifetime.
pub(crate) fn install(app: &AppHandle) {
    let handle = app.clone();
    app.listen_any("recorder://state", move |event| {
        match parse_state(event.payload()) {
            Some(StateUpdate::Show(state)) => native::update(&handle, state),
            Some(StateUpdate::Closed) => {
                NATIVE_CLOSED.store(true, Ordering::SeqCst);
                native::set_visible(&handle, false);
            }
            None => {}
        }
    });
}

/// Build the native pill for a new recorder window, shown iff the Meetings
/// window can't cover for it right now.
pub(crate) fn create_native(app: &AppHandle, visible: bool) {
    NATIVE_CLOSED.store(false, Ordering::SeqCst);
    native::create(app);
    native::set_visible(app, visible);
}

/// Tear the native pill down with its recorder window.
pub(crate) fn destroy_native(app: &AppHandle) {
    native::destroy(app);
}

fn native_should_show(pill_desired: bool, closed: bool) -> bool {
    pill_desired && !closed
}

/// The pill is the fallback recording UI: visible only while the library
/// window (hosting the embedded recorder strip) is absent or minimized.
fn pill_should_show(library_exists: bool, library_minimized: bool) -> bool {
    !library_exists || library_minimized
}

/// Whether the pill should be visible for the app's current window state.
pub(crate) fn should_show_now(app: &AppHandle) -> bool {
    let lib = app.get_webview_window("library");
    let minimized = lib
        .as_ref()
        .map(|l| l.is_minimized().unwrap_or(false))
        .unwrap_or(false);
    pill_should_show(lib.is_some(), minimized)
}

/// The recorder webview is never the UI, but it is created visible (and
/// re-shown before a Resume) because webviews don't reliably resolve
/// getUserMedia for a hidden window. Hide it once capture is running.
fn should_hide_recorder_window(capture_active: bool, is_visible: bool) -> bool {
    capture_active && is_visible
}

/// Keep the pill's visibility in sync with the library window for the
/// lifetime of the recording. Spawned when the waveform window is created.
pub(crate) fn spawn_watcher(app: &AppHandle, initially_shown: bool) {
    let app = app.clone();
    let generation = WATCHER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // `create_native` already showed or hid the pill to match; only push
    // changes when the desired state actually flips.
    let mut last_native = initially_shown;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            // A queued reopen replaced this watcher's window with a new one
            // under the same label between polls; a newer watcher now owns
            // it, so stand down instead of fighting it over visibility.
            if WATCHER_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            // Recording over (window closed/destroyed) — watcher is done.
            let Some(wave) = app.get_webview_window("waveform") else {
                return;
            };
            let capture = app
                .state::<crate::recording_state::RecordingState>()
                .capture_active();
            let shown = native_should_show(should_show_now(&app), NATIVE_CLOSED.load(Ordering::SeqCst));
            if shown != last_native {
                native::set_visible(&app, shown);
                last_native = shown;
            }
            if should_hide_recorder_window(capture, wave.is_visible().unwrap_or(false)) {
                let _ = wave.hide();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_shows_when_the_library_window_is_minimized() {
        assert!(pill_should_show(true, true));
    }

    #[test]
    fn pill_shows_when_the_library_window_is_closed() {
        assert!(pill_should_show(false, false));
    }

    #[test]
    fn pill_hides_when_the_library_window_is_visible() {
        assert!(!pill_should_show(true, false));
    }

    #[test]
    fn the_recorder_window_hides_only_once_capture_runs() {
        // Webviews won't reliably resolve getUserMedia for a hidden window, so
        // it stays on screen (empty, click-through) until capture is running.
        assert!(!should_hide_recorder_window(false, true));
        assert!(should_hide_recorder_window(true, true));
        assert!(!should_hide_recorder_window(true, false));
    }

    fn state(json: serde_json::Value) -> Option<StateUpdate> {
        parse_state(&json.to_string())
    }

    #[test]
    fn parses_each_recorder_phase() {
        for (name, phase) in [
            ("starting", PillPhase::Starting),
            ("recording", PillPhase::Recording),
            ("uploading", PillPhase::Uploading),
            ("success", PillPhase::Success),
            ("failed", PillPhase::Failed),
        ] {
            let parsed = state(serde_json::json!({
                "bars": [0.1, 0.5, 0.2],
                "durationSeconds": 42,
                "isPaused": true,
                "meetingId": 7,
                "localRecordingId": null,
                "phase": name,
            }));
            assert_eq!(
                parsed,
                Some(StateUpdate::Show(PillState {
                    phase,
                    paused: true,
                    duration_s: 42,
                    bars: [0.1, 0.5, 0.2],
                })),
                "{name}"
            );
        }
    }

    #[test]
    fn a_closed_broadcast_hides_the_pill() {
        assert_eq!(
            state(serde_json::json!({ "phase": "closed", "bars": [], "durationSeconds": 3, "isPaused": false })),
            Some(StateUpdate::Closed)
        );
    }

    #[test]
    fn bars_are_padded_truncated_and_clamped_to_three() {
        let bars = |raw: serde_json::Value| match state(serde_json::json!({
            "phase": "recording", "bars": raw, "durationSeconds": 0, "isPaused": false,
        })) {
            Some(StateUpdate::Show(s)) => s.bars,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(bars(serde_json::json!([])), [0.0, 0.0, 0.0]);
        assert_eq!(bars(serde_json::json!([0.4])), [0.4, 0.0, 0.0]);
        assert_eq!(bars(serde_json::json!([-1, 2, 0.3, 0.9])), [0.0, 1.0, 0.3]);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        assert_eq!(
            state(serde_json::json!({ "phase": "starting" })),
            Some(StateUpdate::Show(PillState {
                phase: PillPhase::Starting,
                paused: false,
                duration_s: 0,
                bars: [0.0; 3],
            }))
        );
    }

    #[test]
    fn rejects_malformed_payloads() {
        assert_eq!(parse_state("true"), None);
        assert_eq!(parse_state("not json"), None);
        assert_eq!(state(serde_json::json!({ "durationSeconds": 3 })), None);
        assert_eq!(state(serde_json::json!({ "phase": "exploded" })), None);
    }

    #[test]
    fn action_codes_match_the_swift_abi() {
        assert_eq!(PillAction::from_code(0), Some(PillAction::OpenMeetings));
        assert_eq!(PillAction::from_code(1), Some(PillAction::Pause));
        assert_eq!(PillAction::from_code(2), Some(PillAction::Resume));
        assert_eq!(PillAction::from_code(3), Some(PillAction::Stop));
        assert_eq!(PillAction::from_code(4), Some(PillAction::RetryUpload));
        assert_eq!(PillAction::from_code(5), Some(PillAction::ContinueRecording));
        assert_eq!(PillAction::from_code(6), Some(PillAction::DiscardRecording));
        assert_eq!(PillAction::from_code(7), None);
        assert_eq!(PillAction::from_code(-1), None);
    }

    #[test]
    fn actions_reuse_the_events_the_tray_and_strip_send() {
        assert_eq!(PillAction::Pause.event(), Some("tray://pause-recording"));
        assert_eq!(PillAction::Resume.event(), Some("tray://resume-recording"));
        assert_eq!(PillAction::Stop.event(), Some("tray://stop-recording"));
        assert_eq!(PillAction::RetryUpload.event(), Some("recorder://retry-upload"));
        assert_eq!(PillAction::ContinueRecording.event(), Some("recorder://continue-recording"));
        assert_eq!(PillAction::DiscardRecording.event(), Some("recorder://discard-recording"));
        // Opening Meetings is a window operation, not a recorder event.
        assert_eq!(PillAction::OpenMeetings.event(), None);
    }

    #[test]
    fn a_closed_native_pill_stays_hidden() {
        assert!(native_should_show(true, false));
        assert!(!native_should_show(false, false));
        assert!(!native_should_show(true, true));
    }
}
