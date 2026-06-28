use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub os: &'static str,
    pub local_backend: LocalBackendCapability,
    pub system_audio: UrlCapability,
    pub auto_record: SupportedCapability,
    pub native_share: SupportedCapability,
    pub notification_settings_url: Option<&'static str>,
    pub microphone_settings_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalBackendCapability {
    pub supported: bool,
    pub engine: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlCapability {
    pub supported: bool,
    pub settings_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedCapability {
    pub supported: bool,
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn local_backend() -> LocalBackendCapability {
    if cfg!(target_os = "macos") {
        LocalBackendCapability {
            supported: true,
            engine: Some("swift-mlx"),
        }
    } else if cfg!(target_os = "windows") {
        LocalBackendCapability {
            supported: true,
            engine: Some("cpp-sidecar"),
        }
    } else {
        LocalBackendCapability {
            supported: false,
            engine: None,
        }
    }
}

fn system_audio() -> UrlCapability {
    if cfg!(target_os = "macos") {
        UrlCapability {
            supported: true,
            settings_url: Some(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            ),
        }
    } else if cfg!(target_os = "windows") {
        UrlCapability {
            supported: false,
            settings_url: Some("ms-settings:sound"),
        }
    } else {
        UrlCapability {
            supported: false,
            settings_url: None,
        }
    }
}

pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        os: os_name(),
        local_backend: local_backend(),
        system_audio: system_audio(),
        auto_record: SupportedCapability {
            supported: crate::mic_monitor::is_supported(),
        },
        native_share: SupportedCapability {
            supported: cfg!(target_os = "macos"),
        },
        notification_settings_url: if cfg!(target_os = "macos") {
            Some("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
        } else if cfg!(target_os = "windows") {
            Some("ms-settings:notifications")
        } else {
            None
        },
        microphone_settings_url: if cfg!(target_os = "macos") {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        } else if cfg!(target_os = "windows") {
            Some("ms-settings:privacy-microphone")
        } else {
            None
        },
    }
}

#[tauri::command]
pub fn platform_capabilities() -> PlatformCapabilities {
    capabilities()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_os_name_is_reported() {
        assert!(matches!(os_name(), "macos" | "windows" | "linux"));
    }

    #[test]
    fn windows_settings_urls_are_exact_when_compiled_for_windows() {
        let caps = capabilities();
        if cfg!(target_os = "windows") {
            assert_eq!(caps.microphone_settings_url, Some("ms-settings:privacy-microphone"));
            assert_eq!(caps.notification_settings_url, Some("ms-settings:notifications"));
            assert_eq!(caps.system_audio.settings_url, Some("ms-settings:sound"));
        }
    }
}
