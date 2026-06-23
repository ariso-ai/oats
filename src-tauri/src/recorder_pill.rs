//! Controls when the floating recorder pill ("waveform" window) is visible.
//!
//! While a recording is on-going, the recording UI normally lives in the
//! library window's embedded recorder strip; the pill is the fallback shown
//! only when that strip can't be seen — the library window is minimized or
//! closed. Tauri emits no minimize/restore events, so a watcher task polls
//! and exits once the waveform window is gone.

use tauri::{AppHandle, Emitter, Manager};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

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
}
