use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindowBuilder,
};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../../src/assets/ariso-logo-w.png");

/// Rebuild the tray menu in-place. Called from tray events (main thread)
/// and from the `set_tray_recording` command.
pub fn set_menu(app: &AppHandle, is_recording: bool, is_paused: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let menu = if is_recording {
        build_recording_menu(app, is_paused)
    } else {
        build_idle_menu(app)
    };
    if let Ok(menu) = menu {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_idle_menu(app)?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(TRAY_ICON_BYTES)?)
        .icon_as_template(false)
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "start_recording" => {
                    // Validate session against the API before opening the
                    // recording UI. A locally-stored token may be stale.
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let valid =
                            crate::commands::is_session_valid(&app_async).await;
                        let app_main = app_async.clone();
                        let _ = app_async.run_on_main_thread(move || {
                            if !valid {
                                if let Some(win) =
                                    app_main.get_webview_window("settings")
                                {
                                    let _ = win.show();
                                    let _ = win.set_focus();
                                }
                                let _ = app_main
                                    .emit("tray://show-sign-in-prompt", ());
                                return;
                            }
                            if app_main.get_webview_window("waveform").is_none()
                            {
                                if WebviewWindowBuilder::new(
                                    &app_main,
                                    "waveform",
                                    tauri::WebviewUrl::App("/#/waveform".into()),
                                )
                                .title("")
                                .inner_size(320.0, 56.0)
                                .decorations(false)
                                .always_on_top(true)
                                .resizable(false)
                                .transparent(true)
                                .shadow(false)
                                .skip_taskbar(true)
                                .build()
                                .is_err()
                                {
                                    return;
                                }
                            }
                            set_menu(&app_main, true, false);
                            let _ = app_main.emit("tray://start-recording", ());
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
                    .title("Ariso Settings")
                    .inner_size(450.0, 520.0)
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
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

pub fn build_idle_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let start = MenuItemBuilder::with_id("start_recording", "Start Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Ariso").build(app)?;

    MenuBuilder::new(app)
        .item(&start)
        .separator()
        .item(&settings)
        .separator()
        .item(&quit)
        .build()
}

pub fn build_recording_menu(app: &AppHandle, is_paused: bool) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let pause_or_resume = if is_paused {
        MenuItemBuilder::with_id("resume_recording", "Resume Recording").build(app)?
    } else {
        MenuItemBuilder::with_id("pause_recording", "Pause Recording").build(app)?
    };
    let stop = MenuItemBuilder::with_id("stop_recording", "Stop Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;

    // Quit is intentionally omitted while recording to prevent
    // losing the current recording and skipping the upload flow.
    MenuBuilder::new(app)
        .item(&pause_or_resume)
        .item(&stop)
        .separator()
        .item(&settings)
        .build()
}
