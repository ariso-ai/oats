//! The `oats://` URL scheme, registered in `Info.plist` (macOS only).
//!
//! Its one job is handing control back from the browser: the loopback page that
//! ends a sign-in (or Calendar connect) redirects to `oats://return`, so the
//! browser offers to open oats and the window that started the flow comes
//! forward. Any web page can open an `oats://` URL, so the handler treats the
//! URL as untrusted: it never reads the host, path or query, and does nothing
//! but surface a window.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::Mutex;

/// The scheme registered under `CFBundleURLSchemes` in `src-tauri/Info.plist`.
pub(crate) const APP_URL_SCHEME: &str = "oats";

/// Label of the webview that started the most recent browser flow whose
/// callback arrived: the window to bring forward when the browser hands back.
static RETURN_WINDOW: Mutex<Option<String>> = Mutex::new(None);

/// Remember `label` as the window to surface on the next `oats://` open.
pub(crate) fn remember_return_window(label: &str) {
    *RETURN_WINDOW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(label.to_string());
}

/// Take the remembered window: one hand-back per completed flow, so a later
/// stray `oats://` open falls back to the Meetings window.
fn take_return_window() -> Option<String> {
    RETURN_WINDOW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// Whether `url` is one of ours. Only the scheme is inspected.
pub(crate) fn is_app_url(url: &tauri::Url) -> bool {
    url.scheme().eq_ignore_ascii_case(APP_URL_SCHEME)
}

/// Handle `RunEvent::Opened`: when any URL is an `oats://` one, bring the
/// window that started the browser flow forward — or the Meetings window when
/// that one is gone (or nothing is pending).
#[cfg(target_os = "macos")]
pub(crate) fn handle_opened_urls(app: &tauri::AppHandle, urls: &[tauri::Url]) {
    use tauri::Manager;

    if !urls.iter().any(is_app_url) {
        return;
    }
    let target = take_return_window().and_then(|label| app.get_webview_window(&label));
    let surfaced = match target {
        Some(win) => {
            let _ = win.unminimize();
            win.show()
                .and_then(|_| win.set_focus())
                .map_err(|e| e.to_string())
        }
        None => crate::commands::open_library_window(app),
    };
    if let Err(e) = surfaced {
        eprintln!("Failed to surface a window for an {APP_URL_SCHEME}:// open: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> tauri::Url {
        tauri::Url::parse(s).expect("test url parses")
    }

    #[test]
    fn is_app_url_matches_only_the_oats_scheme() {
        assert!(is_app_url(&url("oats://return")));
        assert!(is_app_url(&url("OATS://return")));
        assert!(is_app_url(&url("oats:anything?token=ignored")));
        assert!(!is_app_url(&url("https://oats/return")));
        assert!(!is_app_url(&url("oatsx://return")));
        assert!(!is_app_url(&url("file:///tmp/oats")));
    }

    #[test]
    fn return_window_is_handed_back_once() {
        take_return_window();
        remember_return_window("settings");
        remember_return_window("onboarding");
        assert_eq!(take_return_window().as_deref(), Some("onboarding"));
        assert_eq!(take_return_window(), None);
    }
}
