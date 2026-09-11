//! The floating recorder pill: when it is visible, and — where the platform
//! draws it natively — the bridge between it and the recorder.
//!
//! While a recording is on-going, the recording UI normally lives in the
//! library window's embedded recorder strip; the pill is the fallback shown
//! only when that strip can't be seen — the library window is minimized or
//! closed. Tauri emits no minimize/restore events, so a watcher task polls
//! and exits once the waveform window is gone.
//!
//! The recording session itself always runs in the "waveform" webview. Where
//! [`NATIVE`] is set, that webview paints nothing and stays hidden once
//! capture starts; a native pill renders its `recorder://state` broadcasts and
//! turns clicks back into the events the webview pill used to handle.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::{AppHandle, Emitter, Listener, Manager, PhysicalPosition, WebviewWindow};

#[cfg(any(target_os = "windows", test))]
mod layout;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as native;

/// Platforms without a native pill keep painting it in the webview.
#[cfg(not(target_os = "macos"))]
mod native {
    pub(super) fn create(_app: &tauri::AppHandle) {}
    pub(super) fn update(_app: &tauri::AppHandle, _state: super::PillState) {}
    pub(super) fn set_visible(_app: &tauri::AppHandle, _visible: bool) {}
    pub(super) fn destroy(_app: &tauri::AppHandle) {}
    pub(super) fn is_available() -> bool {
        false
    }
}

/// Whether this platform draws the pill natively, leaving the "waveform"
/// webview as a headless recorder host.
pub(crate) const NATIVE: bool = cfg!(target_os = "macos");

/// Whether the native pill is actually usable right now: the platform draws
/// it natively *and* the linked native library speaks the expected ABI. Falls
/// back to [`NATIVE`] = `false` behavior (the webview paints the pill itself)
/// when a mismatched build leaves the native side unable to draw anything.
pub(crate) fn native_available() -> bool {
    NATIVE && native::is_available()
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
    if !NATIVE {
        return;
    }
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

/// With a native pill the webview is never the UI: it is on screen only until
/// capture starts (see `visibility_action`), then hidden for good.
fn webview_should_show(pill_desired: bool, native: bool) -> bool {
    pill_desired && !native
}

fn native_should_show(pill_desired: bool, closed: bool) -> bool {
    pill_desired && !closed
}

/// Pill ("waveform") window size in CSS px — must match the `inner_size` used
/// when the window is built (see `open_waveform_window`). Kept here so the
/// right-edge docking math and the window dimensions stay in sync.
pub(crate) const PILL_W: f64 = 92.0;
pub(crate) const PILL_H: f64 = 284.0;
/// Gap from the screen's right edge, in CSS px.
const PILL_MARGIN: f64 = 16.0;

/// Physical-pixel top-left for the pill docked to a monitor's right edge,
/// vertically centered. Pure so the edge math is unit-tested without a window.
fn pill_dock_position(monitor_pos: (i32, i32), monitor_size: (u32, u32), scale: f64) -> (i32, i32) {
    let win_w = (PILL_W * scale).round() as i32;
    let win_h = (PILL_H * scale).round() as i32;
    let margin = (PILL_MARGIN * scale).round() as i32;
    let x = monitor_pos.0 + monitor_size.0 as i32 - win_w - margin;
    let y = monitor_pos.1 + (monitor_size.1 as i32 - win_h) / 2;
    (x, y)
}

/// Dock the pill to the right edge of the primary screen, vertically centered,
/// rather than wherever the OS first placed it (≈ mid-screen). Called both when
/// the pill is born as the visible UI and each time the watcher reveals it
/// (e.g. the meetings window was minimized mid-recording).
pub(crate) fn dock_to_right_edge(win: &WebviewWindow) {
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let msize = monitor.size();
        let mpos = monitor.position();
        let (x, y) = pill_dock_position(
            (mpos.x, mpos.y),
            (msize.width, msize.height),
            monitor.scale_factor(),
        );
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
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

/// What the watcher should do this tick: `Some(true)` show, `Some(false)`
/// hide, `None` leave as-is. Hiding is deferred until capture has started:
/// WebKit never resolves getUserMedia for a hidden window, so hiding the
/// freshly-created (visible) recorder too early would stall the recording.
fn visibility_action(should_show: bool, capture_active: bool, is_visible: bool) -> Option<bool> {
    if should_show != is_visible && (should_show || capture_active) {
        return Some(should_show);
    }
    None
}

/// Keep the pill's visibility in sync with the library window for the
/// lifetime of the recording. Spawned when the waveform window is created.
pub(crate) fn spawn_watcher(app: &AppHandle, initially_shown: bool) {
    let app = app.clone();
    let generation = WATCHER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // Native availability can't change mid-process (it's a one-time ABI
    // check), so snapshot it once rather than re-checking every poll.
    let native = native_available();
    // The waveform window was born painting itself iff it should show (see
    // `waveform_url`'s pillHidden flag), and `create_native` showed or hid the
    // native pill the same way; mirror that so we only push changes when the
    // desired state actually flips.
    let mut last_desired = initially_shown;
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
            let desired = should_show_now(&app);
            if native {
                let native_visible = native_should_show(desired, NATIVE_CLOSED.load(Ordering::SeqCst));
                if native_visible != last_native {
                    native::set_visible(&app, native_visible);
                    last_native = native_visible;
                }
            } else if desired != last_desired {
                // Tell the waveform window whether to paint the pill. Decoupled
                // from show()/hide() (which waits on capture): painting an
                // off-screen or hidden window is a no-op, but it must be painted
                // the instant the window is shown again, so the paint state
                // tracks `desired` directly.
                let _ = app.emit_to("waveform", "recorder://pill-visible", desired);
                last_desired = desired;
            }
            let visible = wave.is_visible().unwrap_or(desired);
            match visibility_action(webview_should_show(desired, native), capture, visible) {
                Some(true) => {
                    // Re-dock to the right edge before revealing: the pill was
                    // born at the OS default spot when the meetings window owned
                    // the UI, so show it docked rather than mid-screen.
                    dock_to_right_edge(&wave);
                    let _ = wave.show();
                }
                Some(false) => {
                    let _ = wave.hide();
                }
                None => {}
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
    fn hiding_waits_for_capture_to_start() {
        // WebKit won't resolve getUserMedia for a hidden window, so the pill
        // must stay visible until capture is running.
        assert_eq!(visibility_action(false, false, true), None);
        assert_eq!(visibility_action(false, true, true), Some(false));
    }

    #[test]
    fn showing_never_waits() {
        assert_eq!(visibility_action(true, false, false), Some(true));
        assert_eq!(visibility_action(true, true, false), Some(true));
    }

    #[test]
    fn steady_states_do_nothing() {
        assert_eq!(visibility_action(true, true, true), None);
        assert_eq!(visibility_action(false, true, false), None);
        assert_eq!(visibility_action(false, false, false), None);
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
    fn native_pill_never_wants_the_webview_on_screen() {
        // The webview is only kept visible until capture starts; after that it
        // hides even while the Meetings window is closed.
        assert!(!webview_should_show(true, true));
        assert!(!webview_should_show(false, true));
        assert!(webview_should_show(true, false));
        assert!(!webview_should_show(false, false));
    }

    #[test]
    fn a_closed_native_pill_stays_hidden() {
        assert!(native_should_show(true, false));
        assert!(!native_should_show(false, false));
        assert!(!native_should_show(true, true));
    }

    #[test]
    fn docks_against_the_monitor_right_edge_vertically_centered() {
        // 1920x1080 primary monitor at the origin, no HiDPI scaling.
        let (x, y) = pill_dock_position((0, 0), (1920, 1080), 1.0);
        assert_eq!(x, 1920 - PILL_W as i32 - PILL_MARGIN as i32); // 16px gap from the right
        assert_eq!(y, (1080 - PILL_H as i32) / 2); // vertically centered
    }

    #[test]
    fn dock_position_respects_scale_and_monitor_offset() {
        // A 2x monitor positioned to the right of a primary one (offset origin).
        let (x, y) = pill_dock_position((1920, 0), (2560, 1440), 2.0);
        let win_w = (PILL_W * 2.0) as i32;
        let margin = (PILL_MARGIN * 2.0) as i32;
        assert_eq!(x, 1920 + 2560 - win_w - margin);
        assert_eq!(y, (1440 - (PILL_H * 2.0) as i32) / 2);
    }
}
