//! Controls when the floating recorder pill ("waveform" window) is visible.
//!
//! While a recording is on-going, the recording UI normally lives in the
//! library window's embedded recorder strip; the pill is the fallback shown
//! only when that strip can't be seen — the library window is minimized or
//! closed. Tauri emits no minimize/restore events, so a watcher task polls
//! and exits once the waveform window is gone.

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

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
pub(crate) fn spawn_watcher(app: &AppHandle) {
    let app = app.clone();
    // The waveform window was born painting itself iff it should currently show
    // (see `waveform_url`'s pillHidden flag); mirror that so we only push paint
    // changes when the desired state actually flips.
    let mut last_desired = should_show_now(&app);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            // Recording over (window closed/destroyed) — watcher is done.
            let Some(wave) = app.get_webview_window("waveform") else {
                return;
            };
            let capture = app
                .state::<crate::recording_state::RecordingState>()
                .capture_active();
            let desired = should_show_now(&app);
            // Tell the waveform window whether to paint the pill. Decoupled from
            // show()/hide() (which waits on capture): painting an off-screen or
            // hidden window is a no-op, but it must be painted the instant the
            // window is shown again, so the paint state tracks `desired` directly.
            if desired != last_desired {
                let _ = app.emit_to("waveform", "recorder://pill-visible", desired);
                last_desired = desired;
            }
            let visible = wave.is_visible().unwrap_or(desired);
            match visibility_action(desired, capture, visible) {
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
