//! Native platform capability boundary exposed to the webview.
//!
//! Compile-time support belongs here so product code does not infer features
//! from user-agent strings. These values describe what this build can attempt;
//! they are not runtime permission checks and do not imply that models are
//! installed or OS access has been granted.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// One snapshot of platform support consumed by Settings, recording, and other
/// frontend workflows. Keeping related flags together lets a new platform enter
/// through one typed contract instead of scattered conditional branches.
pub struct PlatformCapabilities {
    /// Stable product-facing OS family, intentionally coarser than Rust target triples.
    pub os: &'static str,
    /// Identifies both availability and the implementation family used for diagnostics.
    pub local_backend: LocalBackendCapability,
    /// Couples feature support with the nearest OS settings destination.
    pub system_audio: UrlCapability,
    /// Reports native external-capture detection, not the user's enabled preference.
    pub auto_record: SupportedCapability,
    /// Reports whether the host has a native sharing implementation.
    pub native_share: SupportedCapability,
    /// Deep links may exist even when permission has already been denied.
    pub notification_settings_url: Option<&'static str>,
    /// Native capture owns microphone access; this is the nearest OS privacy pane.
    pub microphone_settings_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Separates the product promise of Local mode from its platform engine. The
/// frontend should branch on support; the engine label is descriptive and must
/// not become a second dispatch mechanism outside native code.
pub struct LocalBackendCapability {
    pub supported: bool,
    pub engine: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Models features whose support and settings navigation are related but not
/// equivalent. A settings destination helps diagnose device configuration; it
/// is not itself the implementation or a permission result.
pub struct UrlCapability {
    pub supported: bool,
    pub settings_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Gives simple native-only features the same extensible object shape as richer
/// capabilities, avoiding another frontend contract when metadata is added.
pub struct SupportedCapability {
    pub supported: bool,
}

/// Reduces target triples to the OS vocabulary shared with TypeScript. Unknown
/// desktop targets use the conservative Linux bucket until explicitly modeled.
fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// Declares which sidecar family fulfills Local mode on this build. This does
/// not inspect downloaded files; `model_manager` remains the readiness authority.
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

/// Distinguishes implementation support from discoverability of OS controls.
/// A settings URL is assistance for the user, not evidence that capture exists.
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
            supported: true,
            settings_url: Some("ms-settings:sound"),
        }
    } else {
        UrlCapability {
            supported: false,
            settings_url: None,
        }
    }
}

/// Assembles a self-consistent capability snapshot from native compile-time
/// truth and subsystem probes. No filesystem or permission prompts occur here,
/// so callers can safely use it during window bootstrap.
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

/// Exposes the snapshot as the sole Tauri IPC entry point. The pure constructor
/// stays separate so native callers and tests do not need an application handle.
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

    #[test]
    fn windows_local_backend_uses_cpp_sidecar() {
        let caps = capabilities();
        if cfg!(target_os = "windows") {
            assert!(caps.local_backend.supported);
            assert_eq!(caps.local_backend.engine, Some("cpp-sidecar"));
        }
    }

    #[test]
    fn windows_native_recording_capabilities_are_enabled() {
        let caps = capabilities();
        if cfg!(target_os = "windows") {
            assert!(caps.system_audio.supported);
            assert!(caps.auto_record.supported);
        }
    }
}
