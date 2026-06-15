use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindowBuilder,
};

// Appearance-aware tray icons (NOT template images, so the fill color shows).
// Both are 128x128 so the icon renders at the same size in either appearance.
// The dark-mode mark has a yellow outline (visible on a dark menu bar).
// `apply_theme` swaps between them on system-appearance changes.
const TRAY_ICON_LIGHT: &[u8] = include_bytes!("../../src/assets/oats-tray.png");
const TRAY_ICON_DARK: &[u8] = include_bytes!("../../src/assets/oats-tray-dark.png");

fn tray_icon_bytes(theme: tauri::Theme) -> &'static [u8] {
    match theme {
        tauri::Theme::Dark => TRAY_ICON_DARK,
        _ => TRAY_ICON_LIGHT,
    }
}

/// Swap the tray icon to match the current menu-bar appearance. Called once
/// after the main window exists (to set the correct initial icon) and again on
/// every `ThemeChanged` event.
pub fn apply_theme(app: &AppHandle, theme: tauri::Theme) {
    let Some(tray) = app.tray_by_id("main") else { return };
    if let Ok(icon) = Image::from_bytes(tray_icon_bytes(theme)) {
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

    TrayIconBuilder::with_id("main")
        // Default to the light-mode icon; main.rs corrects this to the actual
        // system appearance once the main window exists, then keeps it in sync.
        .icon(Image::from_bytes(TRAY_ICON_LIGHT)?)
        .icon_as_template(false)
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "start_recording" => {
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let backend = crate::commands::active_backend(&app_async);

                        if backend == "local" {
                            let root = match crate::storage::ariso_root() {
                                Ok(r) => r,
                                Err(_) => return,
                            };
                            let ready = crate::model_manager::is_ready(&root);
                            let app_main = app_async.clone();
                            let _ = app_async.run_on_main_thread(move || {
                                if !ready {
                                    if let Some(win) = app_main.get_webview_window("settings") {
                                        let _ = win.show();
                                        let _ = win.set_focus();
                                    }
                                    let _ = app_main.emit("tray://show-model-prompt", ());
                                    return;
                                }
                                let _ = crate::commands::open_waveform_window(&app_main, None, false);
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
                            let _ = crate::commands::open_meeting_picker_window(&app_main);
                        });
                    });
                }
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
                    } else if let Ok(win) = WebviewWindowBuilder::new(
                        app,
                        "settings",
                        tauri::WebviewUrl::App("/#/settings".into()),
                    )
                    .title("oats Settings")
                    .inner_size(450.0, 800.0)
                    .resizable(false)
                    .center()
                    .skip_taskbar(true)
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
                "library" => {
                    // The Meetings window is backend-aware: its list is
                    // populated from the active backend (Ariso server meetings
                    // or local recordings), so open the in-app window for both
                    // rather than sending Ariso users out to the browser.
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::create_library_window(app_async).await;
                    });
                }
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
        })
        .build(app)?;

    Ok(())
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
    let check_updates = MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;

    // Quit is intentionally omitted while recording to prevent
    // losing the current recording and skipping the upload flow.
    MenuBuilder::new(app)
        .item(&pause_or_resume)
        .item(&stop)
        .separator()
        .item(&settings)
        .item(&check_updates)
        .build()
}
