use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

// The menu-bar icon is a macOS template image: AppKit uses the PNG alpha mask
// and tints it for the current menu-bar material, matching system status items.
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

/// Resolves the platform-appropriate icon bytes behind one tray construction
/// path. macOS keeps template semantics, while Windows receives already-colored
/// pixels because its notification area does not honor AppKit-style masks.
fn tray_icon(theme: tauri::Theme) -> tauri::Result<Image<'static>> {
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
        let _ = theme;
        Image::from_bytes(TRAY_ICON_TEMPLATE)
    }
}

/// Re-applies the platform icon after startup and theme notifications. macOS
/// reuses one template mask and delegates tinting to AppKit; Windows swaps the
/// concrete asset selected above. Menu state is intentionally untouched.
pub fn apply_theme(app: &AppHandle, theme: tauri::Theme) {
    let Some(tray) = app.tray_by_id("main") else { return };
    if let Ok(icon) = tray_icon(theme) {
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_with_as_template(Some(icon), true);
        #[cfg(not(target_os = "macos"))]
        let _ = tray.set_icon(Some(icon));
    }
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
        .icon(tray_icon(tauri::Theme::Light)?)
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

                        let valid = crate::commands::is_session_valid(&app_async).await;
                        let app_main = app_async.clone();
                        let _ = app_async.run_on_main_thread(move || {
                            if !valid {
                                if let Some(win) = app_main.get_webview_window("settings") {
                                    let _ = win.show();
                                    let _ = win.set_focus();
                                }
                                let _ = app_main.emit("tray://show-sign-in-prompt", ());
                                return;
                            }
                            let _ = crate::commands::open_waveform_window(
                                &app_main,
                                Some(meeting_id),
                                None,
                                false,
                                false,
                            );
                        });
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
                    if let Some(win) = app.get_webview_window("settings") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    } else if let Ok(win) = crate::window_style::settings_window_builder(app)
                        .build()
                    {
                        let win_clone = win.clone();
                        win.on_window_event(move |event| {
                            if let tauri::WindowEvent::CloseRequested { api, .. } =
                                event
                            {
                                api.prevent_close();
                                let _ = win_clone.hide();
                            }
                        });
                    }
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
    let builder = builder
        // Mark it as a template so AppKit tints it alongside the other menu-bar
        // status icons instead of preserving the brand colors.
        .icon_as_template(true);
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

        if backend == "local" {
            let ready = crate::commands::local_models_ready();
            let app_main = app_async.clone();
            let _ = app_async.run_on_main_thread(move || {
                if !ready {
                    crate::commands::surface_model_download(&app_main);
                    return;
                }
                let _ = crate::commands::open_waveform_window(
                    &app_main, None, None, false, false,
                );
            });
            return;
        }

        // Ariso (default): existing session gate + meeting-picker.
        let valid = crate::commands::is_session_valid(&app_async).await;
        let app_main = app_async.clone();
        let _ = app_async.run_on_main_thread(move || {
            if !valid {
                if let Some(win) = app_main.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                let _ = app_main.emit("tray://show-sign-in-prompt", ());
                return;
            }
            let _ = crate::commands::open_meeting_picker_window(&app_main, None);
        });
    });
}

/// Open the backend-aware Meetings window from native menus. This stays next
/// to `start_recording` because both are alternative entry points into the
/// same frontend workflows, not separate tray-only features.
pub fn open_library(app: &AppHandle) {
    let app_async = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::commands::create_library_window(app_async).await;
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
