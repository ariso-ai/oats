import { getPlatformCapabilities, type PlatformCapabilities } from '../tauri';

/** Supplies a conservative OS hint only when native IPC is unavailable, such as
 * unit tests or a browser preview. Production support decisions are overwritten
 * by the Rust capability snapshot and must not rely on this user-agent probe. */
function browserOs(): PlatformCapabilities['os'] {
  if (typeof navigator === 'undefined') return 'linux';
  const platform = navigator.platform.toLowerCase();
  const userAgent = navigator.userAgent.toLowerCase();
  if (platform.includes('mac') || userAgent.includes('mac')) return 'macos';
  if (platform.includes('win') || userAgent.includes('windows')) return 'windows';
  return 'linux';
}

/** Keeps non-Tauri rendering functional without pretending to know permission
 * or model readiness. The fallback mirrors the currently shipped feature matrix
 * so Settings can render before the asynchronous native snapshot arrives. */
export function defaultPlatformCapabilities(): PlatformCapabilities {
  const os = browserOs();
  const isMac = os === 'macos';
  const isWindows = os === 'windows';
  return {
    os,
    localBackend: {
      supported: isMac || isWindows,
      engine: isMac ? 'swift-mlx' : isWindows ? 'cpp-sidecar' : null,
    },
    systemAudio: {
      supported: isMac,
      settingsUrl: isMac
        ? 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture'
        : isWindows
          ? 'ms-settings:sound'
          : null,
    },
    autoRecord: { supported: isMac },
    nativeShare: { supported: isMac },
    notificationSettingsUrl: isMac
      ? 'x-apple.systempreferences:com.apple.Notifications-Settings.extension'
      : isWindows
        ? 'ms-settings:notifications'
        : null,
    microphoneSettingsUrl: isMac
      ? 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
      : isWindows
        ? 'ms-settings:privacy-microphone'
        : null,
  };
}

// A capability snapshot is immutable for the lifetime of one app binary, so one
// promise per webview avoids repeated IPC while still sharing an in-flight call.
// Windows in separate webviews have their own cache; no cross-window state is
// required because every native response is derived from the same build.
let cached: Promise<PlatformCapabilities> | null = null;

/** Returns native capabilities when possible and a render-safe fallback when the
 * bridge is absent or startup fails. Consumers therefore degrade features
 * instead of aborting window initialization on an IPC error. */
export function loadPlatformCapabilities(): Promise<PlatformCapabilities> {
  if (!cached) {
    try {
      cached = getPlatformCapabilities().catch(() => defaultPlatformCapabilities());
    } catch {
      cached = Promise.resolve(defaultPlatformCapabilities());
    }
  }
  return cached;
}

/** Exists for tests and development reloads that need a fresh IPC attempt. App
 * code should treat capabilities as process-static rather than mutable state. */
export function resetPlatformCapabilitiesCache(): void {
  cached = null;
}
