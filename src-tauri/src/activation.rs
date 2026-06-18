//! macOS Dock / Stage Manager visibility.
//!
//! Oats launches as an Accessory app (`LSUIElement`) so it lives only in the
//! menu bar — no Dock icon. But macOS Stage Manager only shows apps whose
//! activation policy is `Regular`, so a pure Accessory app never appears in the
//! Stage Manager strip on the side. To get both behaviors, the activation
//! policy tracks the visible windows: `Regular` (Dock icon + Stage Manager)
//! whenever a real app window is on screen, and back to `Accessory`
//! (menu-bar-only) once they are all hidden or closed.

/// Windows that must never promote the app to a Dock-visible state: the hidden
/// bootstrap window and the always-on-top, chrome-less recorder pill.
#[cfg(target_os = "macos")]
const UTILITY_WINDOWS: &[&str] = &["main", "waveform"];

/// Recompute the activation policy from the currently-visible windows. Cheap
/// and idempotent — safe to call on every window focus/close/destroy event.
#[cfg(target_os = "macos")]
pub fn refresh(app: &tauri::AppHandle) {
    use tauri::{ActivationPolicy, Manager};

    let has_real_window = app.webview_windows().iter().any(|(label, win)| {
        !UTILITY_WINDOWS.contains(&label.as_str()) && win.is_visible().unwrap_or(false)
    });

    let policy = if has_real_window {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    };

    if let Err(e) = app.set_activation_policy(policy) {
        eprintln!("Failed to set activation policy: {e}");
    }
}

/// No-op on non-macOS platforms; the Dock / Stage Manager have no equivalent.
#[cfg(not(target_os = "macos"))]
pub fn refresh(_app: &tauri::AppHandle) {}
