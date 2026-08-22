use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

// The idle menu-bar icon is unchanged from before issue #249: a macOS template
// image whose alpha mask AppKit tints for the current menu-bar material, so it
// matches the other system status items.
#[cfg(not(target_os = "windows"))]
const TRAY_ICON_TEMPLATE: &[u8] = include_bytes!("../../src/assets/oats-tray.png");
// Windows does not reliably template-tint tray icons, so use concrete color
// assets and swap them on theme changes.
#[cfg(target_os = "windows")]
/// Concrete icon selected when the Windows shell reports a light theme; the
/// asset, rather than shell tinting, owns the required contrast.
const TRAY_ICON_WINDOWS_LIGHT: &[u8] = include_bytes!("../../src/assets/oats-tray-light.png");
#[cfg(target_os = "windows")]
/// Companion asset for a dark Windows shell theme. Keeping it bundled avoids
/// runtime image manipulation and preserves predictable alpha rendering.
const TRAY_ICON_WINDOWS_DARK: &[u8] = include_bytes!("../../src/assets/oats-tray-dark.png");

// Recording is the *only* state that swaps the icon (issue #249): the full
// color logo makes an active capture obvious at a glance. These are concrete
// color assets on every platform — templating them would flatten the brand
// colors to the same silhouette as idle and erase the difference they exist to
// convey — so each appearance gets the artwork drawn for it.
//
// They are `oats-dark.png` / `oats-light.png` re-laid-out onto the same
// 128x128 full-bleed canvas as `oats-tray.png`. The tray sizes an icon to an
// 18pt height and derives its width from the aspect ratio, so the app-sized
// logos (1332x1160, with wide transparent margins) would have rendered a
// visibly smaller mark inside a wider status item. Matching the idle canvas
// keeps the menu-bar icon exactly the same size in both states.
const TRAY_ICON_RECORDING_DARK: &[u8] =
    include_bytes!("../../src/assets/oats-tray-recording-dark.png");
const TRAY_ICON_RECORDING_LIGHT: &[u8] =
    include_bytes!("../../src/assets/oats-tray-recording-light.png");

/// Last appearance the shell reported. Recording starts and stops long after
/// the theme event, so the icon swap needs the theme cached here rather than
/// re-querying a window that may be hidden or already gone.
static TRAY_THEME_IS_DARK: AtomicBool = AtomicBool::new(false);
/// Whether the tray is currently showing a recording. Cached for the mirror
/// case: a theme change mid-recording must redraw the *recording* icon.
static TRAY_IS_RECORDING: AtomicBool = AtomicBool::new(false);

/// Resolves the icon bytes for one (recording, appearance) combination behind a
/// single tray construction path. Recording picks the appearance-specific color
/// logo; idle keeps the platform behavior it has always had — one template mask
/// on macOS, a concrete per-theme asset on Windows.
fn tray_icon(theme: tauri::Theme, is_recording: bool) -> tauri::Result<Image<'static>> {
    if is_recording {
        let bytes = match theme {
            tauri::Theme::Dark => TRAY_ICON_RECORDING_DARK,
            _ => TRAY_ICON_RECORDING_LIGHT,
        };
        return Image::from_bytes(bytes);
    }

    #[cfg(target_os = "windows")]
    {
        let bytes = match theme {
            tauri::Theme::Dark => TRAY_ICON_WINDOWS_DARK,
            _ => TRAY_ICON_WINDOWS_LIGHT,
        };
        return Image::from_bytes(bytes);
    }

    #[cfg(not(target_os = "windows"))]
    {
        Image::from_bytes(TRAY_ICON_TEMPLATE)
    }
}

fn cached_theme() -> tauri::Theme {
    if TRAY_THEME_IS_DARK.load(Ordering::Relaxed) {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    }
}

/// Push the icon for the given state onto the live tray. On macOS the idle mark
/// stays a template so AppKit tints it like every other status item; only the
/// recording logo turns templating off, because it carries its own color.
fn set_icon(app: &AppHandle, theme: tauri::Theme, is_recording: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    if let Ok(icon) = tray_icon(theme, is_recording) {
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_with_as_template(Some(icon), !is_recording);
        #[cfg(not(target_os = "macos"))]
        let _ = tray.set_icon(Some(icon));
    }
}

/// Re-applies the platform icon after startup and theme notifications, keeping
/// whichever recording state the tray is already in. Menu state is
/// intentionally untouched.
pub fn apply_theme(app: &AppHandle, theme: tauri::Theme) {
    TRAY_THEME_IS_DARK.store(matches!(theme, tauri::Theme::Dark), Ordering::Relaxed);
    set_icon(app, theme, TRAY_IS_RECORDING.load(Ordering::Relaxed));
}

/// Swap between the idle mark and the recording logo. Driven by `set_menu`,
/// which is the single place recording state reaches the tray, so the icon
/// tracks the same flag the menu is rebuilt from.
///
/// Deliberately unconditional: if the menu fails to build we still move the
/// icon, because a stale menu is a wrong list of actions while a stale icon
/// claims we are not recording when we are — exactly what this icon exists to
/// signal. Truthful indicator beats matching indicators.
fn apply_recording(app: &AppHandle, is_recording: bool) {
    TRAY_IS_RECORDING.store(is_recording, Ordering::Relaxed);
    set_icon(app, cached_theme(), is_recording);
}

/// Rebuild the tray menu in-place. Called from tray events (main thread)
/// and from the `set_tray_recording` command.
pub fn set_menu(app: &AppHandle, is_recording: bool, is_paused: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let menu = if is_recording {
        build_recording_menu(app, is_paused)
    } else {
        let featured = app
            .state::<crate::tray_meeting::FeaturedMeetingState>()
            .0
            .lock()
            .unwrap_or_else(|poisoned| {
                // If a previous tray update panicked while holding this lock,
                // keep the last meeting instead of crashing the whole tray.
                eprintln!("tray: FeaturedMeetingState mutex poisoned; recovering");
                poisoned.into_inner()
            })
            .clone();
        build_idle_menu(app, featured.as_ref())
    };
    if let Ok(menu) = menu {
        let _ = tray.set_menu(Some(menu));
    }
    apply_recording(app, is_recording);
    refresh_tray_title(app);
}

/// Render or clear the menu-bar text next to the tray icon. Shows the
/// featured meeting's countdown only when idle; recording (or no upcoming
/// meeting / Local backend / signed out) clears it. macOS-only effect —
/// `set_title` is a no-op elsewhere.
pub fn refresh_tray_title(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let recording = app
        .state::<crate::recording_state::RecordingState>()
        .is_active();
    let featured = app
        .state::<crate::tray_meeting::FeaturedMeetingState>()
        .0
        .lock()
        .unwrap_or_else(|poisoned| {
            // If the shared meeting state was poisoned, use the stored value
            // anyway so the tray can clear or redraw instead of panicking.
            eprintln!("tray: FeaturedMeetingState mutex poisoned; recovering");
            poisoned.into_inner()
        })
        .clone();
    let title = match featured {
        Some(f) if !recording => crate::tray_meeting::format_title_bar(
            f.title.as_deref(),
            f.start_at,
            chrono::Utc::now(),
        ),
        // macOS keeps the previous status-item title when passed `None`; use
        // an explicit empty string for signed-out, Local, and recording states.
        _ => String::new(),
    };
    let _ = tray.set_title(Some(title));
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_idle_menu(app, None)?;

    let builder = TrayIconBuilder::with_id("main")
        .icon(tray_icon(tauri::Theme::Light, false)?)
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "start_recording" => start_recording(app),
                "record_featured" => {
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let meeting_id = app_async
                            .state::<crate::tray_meeting::FeaturedMeetingState>()
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| {
                                // A poisoned lock still contains the selected
                                // meeting; recover it so one-click record works.
                                eprintln!(
                                    "tray: FeaturedMeetingState mutex poisoned; recovering"
                                );
                                poisoned.into_inner()
                            })
                            .as_ref()
                            .map(|f| f.id);
                        let Some(meeting_id) = meeting_id else { return };

                        if let Err(error) = crate::commands::start_recording_window(
                            app_async,
                            Some(meeting_id),
                            None,
                            None,
                        )
                        .await
                        {
                            eprintln!("Failed to start featured meeting recording: {error}");
                        }
                    });
                }
                "pause_recording" => {
                    set_menu(app, true, true);
                    app.emit("tray://pause-recording", ()).ok();
                }
                "resume_recording" => {
                    set_menu(app, true, false);
                    app.emit("tray://resume-recording", ()).ok();
                }
                "stop_recording" => {
                    // Emit stop event first so the frontend can run cleanup
                    // (stopRecording + upload) before the window is destroyed.
                    app.emit("tray://stop-recording", ()).ok();
                    // Switch tray menu back to idle
                    // Switch tray menu back to idle; the waveform window
                    // stays open so the frontend can finish uploading and
                    // show a result before the user closes it.
                    set_menu(app, false, false);
                    // The waveform window closes itself after upload completes.
                }
                "settings" => {
                    let _ = crate::commands::open_settings_window(app);
                }
                "library" => open_library(app),
                "check_updates" => {
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::update_manager::run_check(app_async, true).await;
                    });
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        });
    #[cfg(target_os = "macos")]
    // The tray is built idle, so start as a template and let AppKit tint it
    // alongside the other menu-bar status icons; `set_icon` turns templating
    // off only for the recording logo.
    let builder = builder.icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.icon_as_template(false);
    builder.build(app)?;

    Ok(())
}

/// Start the backend-aware recording flow from any native command surface.
/// Keeping the session/model gates here makes the tray and desktop menu behave
/// identically instead of letting Windows acquire a second recording policy.
pub fn start_recording(app: &AppHandle) {
    let app_async = app.clone();
    tauri::async_runtime::spawn(async move {
        let backend = crate::commands::active_backend(&app_async);
        let result = if backend == "local" {
            crate::commands::start_recording_window(app_async, None, None, None).await
        } else {
            crate::commands::open_meeting_picker(app_async, None).await
        };
        if let Err(error) = result {
            eprintln!("Failed to start recording flow: {error}");
        }
    });
}

/// Open the backend-aware Meetings window from native menus. This stays next
/// to `start_recording` because both are alternative entry points into the
/// same frontend workflows, not separate tray-only features.
pub fn open_library(app: &AppHandle) {
    let app_async = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::commands::create_library_window(app_async).await {
            eprintln!("Failed to present Meetings window: {error}");
        }
    });
}

pub fn build_idle_menu(
    app: &AppHandle,
    featured: Option<&crate::tray_meeting::FeaturedMeeting>,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let mut builder = MenuBuilder::new(app);

    // Featured next meeting: a clickable full-title row that records that
    // meeting, over a disabled (gray) time row. muda has no per-item font
    // control, so a disabled item is the closest native "subtitle".
    if let Some(f) = featured {
        let title = f
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Untitled meeting".to_string());
        let record_featured = MenuItemBuilder::with_id("record_featured", title).build(app)?;
        let time_label = crate::tray_meeting::format_time_range(
            f.start_at.with_timezone(&chrono::Local),
            f.end_at.map(|e| e.with_timezone(&chrono::Local)),
        );
        let time_row = MenuItemBuilder::with_id("featured_time", time_label)
            .enabled(false)
            .build(app)?;
        builder = builder.item(&record_featured).item(&time_row).separator();
    }

    let start = MenuItemBuilder::with_id("start_recording", "Start Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let library = MenuItemBuilder::with_id("library", "Meetings...").build(app)?;
    let check_updates = MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit oats").build(app)?;

    builder
        .item(&start)
        .separator()
        .item(&settings)
        .item(&library)
        .item(&check_updates)
        .separator()
        .item(&quit)
        .build()
}

/// Build the smaller tray menu shown while a recording is running. It exposes
/// only controls that are safe during capture, so users cannot quit mid-upload.
pub fn build_recording_menu(app: &AppHandle, is_paused: bool) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let pause_or_resume = if is_paused {
        MenuItemBuilder::with_id("resume_recording", "Resume Recording").build(app)?
    } else {
        MenuItemBuilder::with_id("pause_recording", "Pause Recording").build(app)?
    };
    let stop = MenuItemBuilder::with_id("stop_recording", "Stop Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let library = MenuItemBuilder::with_id("library", "Meetings...").build(app)?;
    let check_updates = MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;

    // Quit is intentionally omitted while recording to prevent
    // losing the current recording and skipping the upload flow.
    MenuBuilder::new(app)
        .item(&pause_or_resume)
        .item(&stop)
        .separator()
        .item(&settings)
        .item(&library)
        .item(&check_updates)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(theme: tauri::Theme, is_recording: bool) -> Vec<u8> {
        tray_icon(theme, is_recording)
            .expect("tray icon should decode")
            .rgba()
            .to_vec()
    }

    #[test]
    fn every_tray_icon_variant_decodes() {
        for is_recording in [false, true] {
            for theme in [tauri::Theme::Light, tauri::Theme::Dark] {
                assert!(!pixels(theme, is_recording).is_empty());
            }
        }
    }

    #[test]
    fn recording_uses_a_different_icon_than_idle() {
        for theme in [tauri::Theme::Light, tauri::Theme::Dark] {
            assert_ne!(pixels(theme, false), pixels(theme, true));
        }
    }

    /// The menu bar scales a tray icon to a fixed height and takes its width
    /// from the aspect ratio, so any variant with different dimensions would
    /// visibly resize the status item when a recording starts or stops.
    #[test]
    fn every_variant_shares_the_idle_icon_dimensions() {
        let idle = tray_icon(tauri::Theme::Light, false).expect("idle icon should decode");
        let expected = (idle.width(), idle.height());
        for is_recording in [false, true] {
            for theme in [tauri::Theme::Light, tauri::Theme::Dark] {
                let icon = tray_icon(theme, is_recording).expect("tray icon should decode");
                assert_eq!((icon.width(), icon.height()), expected);
            }
        }
    }

    #[test]
    fn recording_has_its_own_light_and_dark_icon() {
        assert_ne!(
            pixels(tauri::Theme::Light, true),
            pixels(tauri::Theme::Dark, true)
        );
    }

    /// The idle mark is deliberately appearance-independent on macOS: one
    /// template mask that AppKit tints. Only recording swaps per appearance.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn idle_icon_does_not_change_with_appearance() {
        assert_eq!(
            pixels(tauri::Theme::Light, false),
            pixels(tauri::Theme::Dark, false)
        );
    }
}
