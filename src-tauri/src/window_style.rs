use tauri::webview::{Color, WebviewWindowBuilder};
use tauri::{Manager, Runtime, TitleBarStyle, WebviewUrl};

const SETTINGS_BACKGROUND: Color = Color(247, 246, 244, 255);

pub(crate) fn settings_window_builder<'a, R, M>(manager: &'a M) -> WebviewWindowBuilder<'a, R, M>
where
    R: Runtime,
    M: Manager<R>,
{
    WebviewWindowBuilder::new(manager, "settings", WebviewUrl::App("/#/settings".into()))
        .title("Oats Settings")
        .title_bar_style(TitleBarStyle::Transparent)
        .background_color(SETTINGS_BACKGROUND)
        .inner_size(450.0, 800.0)
        .resizable(false)
        .center()
        .skip_taskbar(true)
}
