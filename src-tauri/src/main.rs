// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activation;
mod audio_util;
mod audio_capture;
mod mic_capture;
mod commands;
mod meeting_notifications;
mod mic_monitor;
mod recorder_pill;
mod storage;
mod transcribe;
mod model_manager;
mod recording_state;
mod tray;
mod tray_meeting;
mod update_manager;

/// Build the macOS application menu. Mirrors Tauri's default menu (so the
/// standard Edit/Window/View items and their shortcuts still work) but injects
/// the oats logo into the "About oats" panel — without it, the panel falls back
/// to a generic icon in dev builds where no bundle icon is present.
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::image::Image;
    use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};

    let pkg = app.package_info();
    let config = app.config();

    let icon = Image::from_bytes(include_bytes!("../../src/assets/oats-light.png")).ok();
    let about = AboutMetadata {
        name: Some("oats".into()),
        version: Some(pkg.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        icon,
        ..Default::default()
    };

    let app_menu = Submenu::with_items(
        app,
        "oats",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[&PredefinedMenuItem::fullscreen(app, None)?],
    )?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &edit_menu, &view_menu, &window_menu])
}

fn main() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(build_menu)
        .invoke_handler(tauri::generate_handler![
            commands::google_sign_in,
            commands::check_session,
            commands::sign_out,
            commands::api_request,
            commands::upload_file,
            commands::set_tray_recording,
            commands::create_settings_window,
            commands::create_onboarding_window,
            commands::start_recording_window,
            commands::open_meeting_picker,
            commands::put_presigned,
            commands::get_desktop_config,
            commands::list_local_recordings,
            commands::local_recording_status,
            commands::create_library_window,
            commands::get_active_recording_meeting_id,
            commands::read_recording_audio,
            commands::read_recording_file,
            commands::read_recording_note,
            commands::write_recording_note,
            commands::read_recording_note_title,
            commands::write_recording_note_title,
            commands::open_recording_file,
            commands::rename_local_recording,
            commands::buffer_pending_audio,
            commands::discard_pending_audio,
            commands::list_pending_uploads,
            commands::combine_pending_audio,
            commands::fetch_meeting_audio,
            commands::share_text_native,
            transcribe::local_finalize_recording,
            transcribe::retry_local_transcription,
            transcribe::retry_local_notes,
            model_manager::local_model_status,
            model_manager::download_local_stt,
            model_manager::download_local_llm,
            meeting_notifications::sync_meeting_notifications,
            meeting_notifications::stop_meeting_notifications,
            meeting_notifications::show_silence_prompt,
            meeting_notifications::dismiss_silence_prompt,
            meeting_notifications::resolve_silence_prompt,
            meeting_notifications::resize_silence_prompt,
            meeting_notifications::show_meeting_end_prompt,
            meeting_notifications::dismiss_meeting_end_prompt,
            meeting_notifications::resolve_meeting_end_prompt,
            meeting_notifications::resize_meeting_end_prompt,
            meeting_notifications::resolve_meeting_prompt,
            meeting_notifications::resize_meeting_prompt,
            tray_meeting::sync_tray_meeting,
            mic_monitor::sync_auto_record,
            mic_monitor::auto_record_supported,
            mic_monitor::request_mic_monitor_rearm,
            audio_capture::start_system_audio_capture,
            audio_capture::stop_system_audio_capture,
            audio_capture::request_screen_capture_permission,
            audio_capture::check_screen_capture_permission,
            mic_capture::start_microphone_capture,
            mic_capture::stop_microphone_capture,
            mic_capture::request_microphone_permission,
            mic_capture::check_microphone_permission,
            update_manager::update_check,
            update_manager::update_install_and_relaunch,
            update_manager::update_skip_version,
            update_manager::update_snooze,
            update_manager::update_set_auto_check,
            update_manager::update_get_state,
        ])
        .setup(|app| {
            use tauri::{Manager, WebviewWindowBuilder, WebviewUrl};

            // Managed state must exist before the tray is created: tray menu
            // rebuilds and the title refresher read RecordingState and
            // FeaturedMeetingState.
            app.manage(recording_state::RecordingState::new());
            app.manage(tray_meeting::TrayMeetingManager::new());
            app.manage(tray_meeting::FeaturedMeetingState::new());

            tray::create_tray(app.handle())?;

            // Native next-meeting tray orchestrator. Self-gates on Ariso
            // backend + session; re-synced from BootstrapView on SYNC_EVENT.
            tray_meeting::sync(app.handle());

            let initial_state = update_manager::load_state(&app.handle());
            app.manage(update_manager::Manager::new(initial_state));

            // Native meeting-prep notification orchestrator. Owns the Pusher
            // connection in the Rust process (webviews get suspended when
            // hidden). Self-gates on session + the enabled toggle.
            app.manage(mic_monitor::MicMonitorManager::new());
            // Start the auto-record mic monitor (self-gates on OS support + the
            // enabled setting).
            let mic_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                mic_monitor::sync(&mic_handle).await;
            });
            app.manage(meeting_notifications::NotificationManager::new());
            // Install the macOS notification-click delegate on the main thread.
            meeting_notifications::init_native(app.handle());
            let notif_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                meeting_notifications::sync(&notif_handle).await;
            });

            // Hidden bootstrap window — runs JS event listeners
            let main_window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/#/".into()))
                .visible(false)
                .skip_taskbar(true)
                .build()?;

            // Appearance-aware tray icon. The tray is created before any window
            // exists, so set the correct initial icon now and keep it in sync
            // with the system light/dark menu-bar appearance.
            tray::apply_theme(
                app.handle(),
                main_window.theme().unwrap_or(tauri::Theme::Light),
            );
            let theme_handle = app.handle().clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::ThemeChanged(theme) = event {
                    tray::apply_theme(&theme_handle, *theme);
                }
            });

            // Pre-create settings window (hidden) — shown on demand from tray.
            // Intercept close requests so the window hides instead of being
            // destroyed; otherwise re-opening from the tray would do nothing.
            let settings = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("/#/settings".into()))
                .title("Oats Settings")
                .inner_size(450.0, 800.0)
                .resizable(false)
                .center()
                .visible(false)
                .skip_taskbar(true)
                .build()?;

            let settings_clone = settings.clone();
            settings.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = settings_clone.hide();
                    // Settings hides rather than closes, so the global
                    // Destroyed hook never fires — demote here once it's gone.
                    activation::refresh(&settings_clone.app_handle());
                }
            });

            // Background update scheduler: wake every 30 min, but only
            // actually check once per 2h (or on snooze expiry). The
            // initial 10-second delay lets startup finish first.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                loop {
                    update_manager::run_check(app_handle.clone(), false).await;
                    tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
                }
            });

            Ok(())
        });

    #[cfg(all(debug_assertions, feature = "mcp"))]
    {
        let home_dir = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home_dir) = home_dir {
            let socket_path = std::path::PathBuf::from(home_dir).join(".ariso/run/oats-mcp.sock");
            let dir_ready = match socket_path.parent() {
                Some(dir) => match std::fs::create_dir_all(dir) {
                    Ok(()) => true,
                    Err(e) => {
                        eprintln!(
                            "Warning: failed to create MCP socket directory {}: {}. MCP plugin will not be initialized.",
                            dir.display(),
                            e
                        );
                        false
                    }
                },
                None => true,
            };
            if dir_ready {
                builder = builder.plugin(tauri_plugin_mcp::init_with_config(
                    tauri_plugin_mcp::PluginConfig::new("oats".to_string())
                        .start_socket_server(true)
                        .socket_path(socket_path),
                ));
            }
        } else {
            eprintln!(
                "Warning: neither HOME nor USERPROFILE is set; MCP plugin will not be initialized."
            );
        }
    }

    builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            match &_event {
                // Clicking the Dock icon re-activates the app (Reopen).
                // Surface the meetings window — every other window is a hidden
                // utility (bootstrap, settings) or transient (recorder pill).
                tauri::RunEvent::Reopen { .. } => {
                    if let Err(e) = commands::open_library_window(_app) {
                        eprintln!("Failed to open meetings window on dock reopen: {e}");
                    }
                }
                // Keep the Dock / Stage Manager presence in sync with the
                // visible windows: promote to Regular while a real window is up,
                // demote to Accessory once they're all gone. Focused covers
                // show()/set_focus(); Destroyed covers transient closes.
                tauri::RunEvent::WindowEvent { event, .. } => {
                    if matches!(
                        event,
                        tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Destroyed
                    ) {
                        activation::refresh(_app);
                    }
                }
                _ => {}
            }
        });
}
