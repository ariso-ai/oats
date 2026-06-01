// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_capture;
mod commands;
mod meeting_notifications;
mod tray;
mod update_manager;

fn main() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::google_sign_in,
            commands::check_session,
            commands::sign_out,
            commands::api_request,
            commands::upload_file,
            commands::set_tray_recording,
            commands::create_settings_window,
            commands::start_recording_window,
            commands::put_presigned,
            commands::get_desktop_config,
            meeting_notifications::sync_meeting_notifications,
            meeting_notifications::stop_meeting_notifications,
            audio_capture::start_system_audio_capture,
            audio_capture::stop_system_audio_capture,
            update_manager::update_check,
            update_manager::update_install_and_relaunch,
            update_manager::update_skip_version,
            update_manager::update_snooze,
            update_manager::update_set_auto_check,
            update_manager::update_get_state,
        ])
        .setup(|app| {
            use tauri::{Manager, WebviewWindowBuilder, WebviewUrl};

            tray::create_tray(app.handle())?;

            let initial_state = update_manager::load_state(&app.handle());
            app.manage(update_manager::Manager::new(initial_state));

            // Native meeting-prep notification orchestrator. Owns the Pusher
            // connection in the Rust process (webviews get suspended when
            // hidden). Self-gates on session + the enabled toggle.
            app.manage(meeting_notifications::NotificationManager::new());
            // Install the macOS notification-click delegate on the main thread.
            meeting_notifications::init_native(app.handle());
            let notif_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                meeting_notifications::sync(&notif_handle).await;
            });

            // Hidden bootstrap window — runs JS event listeners
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/#/".into()))
                .visible(false)
                .skip_taskbar(true)
                .build()?;

            // Pre-create settings window (hidden) — shown on demand from tray.
            // Intercept close requests so the window hides instead of being
            // destroyed; otherwise re-opening from the tray would do nothing.
            let settings = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("/#/settings".into()))
                .title("Ariso Settings")
                .inner_size(450.0, 520.0)
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
                }
            });

            // Background update scheduler: wake every hour, but only
            // actually check once per 24h (or on snooze expiry). The
            // initial 10-second delay lets startup finish first.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                loop {
                    update_manager::run_check(app_handle.clone(), false).await;
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            });

            Ok(())
        });

    #[cfg(all(debug_assertions, feature = "mcp"))]
    {
        let home_dir = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home_dir) = home_dir {
            let socket_path = std::path::PathBuf::from(home_dir).join(".ariso/run/sage-mcp.sock");
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
                    tauri_plugin_mcp::PluginConfig::new("Ariso".to_string())
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
