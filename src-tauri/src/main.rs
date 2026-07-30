// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activation;
mod audio_util;
mod audio_capture;
mod mic_capture;
mod commands;
mod meeting_notifications;
mod mic_monitor;
mod platform;
mod recorder_pill;
mod storage;
mod transcribe;
mod transcript_normalization;
mod model_manager;
mod recording_state;
mod tray;
mod tray_meeting;
mod update_manager;
mod vault;
mod window_style;

/// Build the native application menu. It preserves the existing macOS menu and
/// adds Windows entry points for workflows that would otherwise exist only in
/// the tray. The custom About metadata also keeps the oats icon in dev builds.
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::image::Image;
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

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

    // Standard macOS "Settings…" item (⌘,). Its id matches the tray's settings
    // menu item so both routes are handled by the same `on_menu_event` arm.
    let settings_item = MenuItem::with_id(
        app,
        "settings",
        "Settings…",
        true,
        Some("Cmd+,"),
    )?;

    let app_menu = Submenu::with_items(
        app,
        "oats",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // Windows users may hide tray icons, so the two primary workflows also
    // live in the conventional application menu. The handlers call the same
    // backend-aware gates as the tray rather than creating menu-only behavior.
    #[cfg(target_os = "windows")]
    let start_recording = MenuItem::with_id(
        app,
        "start_recording",
        "Start Recording",
        true,
        None::<&str>,
    )?;
    #[cfg(target_os = "windows")]
    let library = MenuItem::with_id(app, "library", "Meetings...", true, None::<&str>)?;
    #[cfg(target_os = "windows")]
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &start_recording,
            &library,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
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

    #[cfg(target_os = "windows")]
    {
        Menu::with_items(
            app,
            &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu],
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        Menu::with_items(app, &[&app_menu, &edit_menu, &view_menu, &window_menu])
    }
}

fn main() {
    // reqwest 0.13, tokio-tungstenite, and the updater all use rustls 0.23. Install a
    // single process-wide crypto provider (ring) before any TLS use, otherwise rustls
    // cannot auto-select one and panics at connect time.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls ring crypto provider");

    let builder = tauri::Builder::default();

    // The Windows executable starts as a tray app, so a second shortcut launch
    // must surface the existing Settings window instead of leaving users with
    // another invisible background process. Tauri requires this plugin first.
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        use tauri::Manager;
        // A second shortcut launch returns the user to their existing primary
        // work surface. Before Meetings has been opened, Settings remains the
        // startup surface for this tray-first application.
        if app.get_webview_window("library").is_some() {
            let _ = commands::open_library_window(app);
        } else {
            let _ = commands::open_settings_window(app);
        }
    }));

    #[allow(unused_mut)]
    let mut builder = builder
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .menu(build_menu)
        .on_menu_event(|app, event| {
            // The app menu's "Settings…" (⌘,) opens the same window the tray does.
            match event.id().as_ref() {
                "settings" => {
                    let _ = commands::open_settings_window(app);
                }
                "start_recording" => tray::start_recording(app),
                "library" => tray::open_library(app),
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::google_sign_in,
            commands::cancel_google_sign_in,
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
            commands::get_vault_dir,
            commands::set_vault_dir,
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
            commands::reveal_pending_upload,
            commands::fetch_meeting_audio,
            commands::share_text_native,
            transcribe::local_recording_id_for_start,
            transcribe::local_finalize_recording,
            transcribe::retry_local_transcription,
            transcribe::retry_local_notes,
            model_manager::local_model_status,
            model_manager::download_local_stt,
            model_manager::download_local_llm,
            meeting_notifications::sync_meeting_notifications,
            meeting_notifications::stop_meeting_notifications,
            meeting_notifications::take_pending_meeting_prep,
            platform::platform_capabilities,
            meeting_notifications::show_silence_prompt,
            meeting_notifications::dismiss_silence_prompt,
            meeting_notifications::resolve_silence_prompt,
            meeting_notifications::resize_silence_prompt,
            meeting_notifications::show_meeting_switch_prompt,
            meeting_notifications::dismiss_meeting_switch_prompt,
            meeting_notifications::resolve_meeting_switch_prompt,
            meeting_notifications::resize_meeting_switch_prompt,
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

            // Seed the configured vault directory from the persisted setting so
            // the free `vault_root()` (called deep in the transcription
            // pipeline) resolves it. Empty/missing → default `~/.ariso/vault`.
            {
                use tauri_plugin_store::StoreExt;
                if let Some(dir) = app
                    .store("settings.json")
                    .ok()
                    .and_then(|s| s.get("vaultDir"))
                    .and_then(|v| v.as_str().map(String::from))
                    .filter(|s| !s.is_empty())
                {
                    crate::vault::set_vault_override(std::path::PathBuf::from(dir));
                }
            }
            // One-time upgrade migration MUST run before ensure_vault (which
            // creates `.oats/recordings`). Best-effort: log and continue.
            if let Err(e) = crate::vault::migrate_legacy_recordings() {
                eprintln!("migrate legacy recordings: {e}");
            }
            // Best-effort: create the vault (+ Attachments/, .oats/, .obsidian)
            // up front so it can be opened in Obsidian before the first
            // recording. Write paths also call ensure_vault lazily.
            if let Err(e) = crate::vault::ensure_vault() {
                eprintln!("ensure vault: {e}");
            }

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
            let settings = crate::window_style::settings_window_builder(app)
                .visible(false)
                .build()?;
            if let Err(e) = crate::window_style::install_settings_window_behavior(&settings) {
                eprintln!("install settings window behavior: {e}");
            }

            // Windows has no persistent application menu while every window is
            // hidden, and users commonly hide tray icons. A shortcut launch
            // therefore opens Settings; closing it returns oats to the tray.
            #[cfg(target_os = "windows")]
            {
                let _ = settings.show();
                let _ = settings.set_focus();
            }

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
