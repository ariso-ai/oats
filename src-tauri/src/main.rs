// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_capture;
mod commands;
mod tray;
mod update_manager;

fn main() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::google_sign_in,
            commands::check_session,
            commands::sign_out,
            commands::api_request,
            commands::upload_file,
            commands::set_tray_recording,
            commands::create_waveform_window,
            commands::destroy_waveform_window,
            commands::create_settings_window,
            commands::put_presigned,
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

            Ok(())
        });

    #[cfg(all(debug_assertions, feature = "mcp"))]
    {
        builder = builder.plugin(tauri_plugin_mcp::init_with_config(
            tauri_plugin_mcp::PluginConfig::new("Ariso".to_string())
                .start_socket_server(true)
                .socket_path("/tmp/ariso-mcp.sock".into()),
        ));
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
