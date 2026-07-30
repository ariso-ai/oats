use tauri::webview::{Color, WebviewWindowBuilder};
use tauri::{Manager, Runtime, WebviewUrl};
#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

const SETTINGS_BACKGROUND: Color = Color(247, 246, 244, 255);

pub(crate) fn settings_window_builder<'a, R, M>(manager: &'a M) -> WebviewWindowBuilder<'a, R, M>
where
    R: Runtime,
    M: Manager<R>,
{
    let builder = WebviewWindowBuilder::new(
        manager,
        "settings",
        WebviewUrl::App("/#/settings".into()),
    )
    .title("Oats Settings");
    // Transparent title bars are an AppKit composition detail; retaining native
    // Windows chrome keeps this shared builder valid on every desktop target.
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(TitleBarStyle::Transparent);
    builder
        .background_color(SETTINGS_BACKGROUND)
        .inner_size(450.0, 800.0)
        .resizable(false)
        .center()
        .skip_taskbar(true)
}

/// Apply behavior owned by the Settings window rather than the application.
/// Windows otherwise inherits the app-wide File/Edit/View/Window menu, which is
/// useful in Meetings but consumes space and exposes unrelated actions here.
pub(crate) fn install_settings_window_behavior(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    #[cfg(target_os = "windows")]
    window.remove_menu()?;

    // Keep Settings resident so every launcher can reuse the same webview and
    // its in-progress form state. Application shutdown still destroys it.
    let window_for_close = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_for_close.hide();
            crate::activation::refresh(window_for_close.app_handle());
        }
    });
    Ok(())
}
