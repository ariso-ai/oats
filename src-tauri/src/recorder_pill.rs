//! Controls when the floating recorder pill ("waveform" window) is visible.
//!
//! While a recording is on-going, the recording UI normally lives in the
//! library window's embedded recorder strip; the pill is the fallback shown
//! only when that strip can't be seen — the library window is minimized or
//! closed. Tauri emits no minimize/restore events, so a watcher task polls
//! and exits once the waveform window is gone.

use tauri::{AppHandle, Manager};

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

/// Keep the pill's visibility in sync with the library window for the
/// lifetime of the recording. Spawned when the waveform window is created.
pub(crate) fn spawn_watcher(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            // Recording over (window closed/destroyed) — watcher is done.
            let Some(wave) = app.get_webview_window("waveform") else {
                return;
            };
            let desired = should_show_now(&app);
            let visible = wave.is_visible().unwrap_or(!desired);
            if desired && !visible {
                let _ = wave.show();
            } else if !desired && visible {
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
}
