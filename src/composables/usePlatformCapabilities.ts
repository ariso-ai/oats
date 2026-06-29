import { getPlatformCapabilities, type PlatformCapabilities } from '../tauri';

function browserOs(): PlatformCapabilities['os'] {
  if (typeof navigator === 'undefined') return 'linux';
  const platform = navigator.platform.toLowerCase();
  const userAgent = navigator.userAgent.toLowerCase();
  if (platform.includes('mac') || userAgent.includes('mac')) return 'macos';
  if (platform.includes('win') || userAgent.includes('windows')) return 'windows';
  return 'linux';
}

export function defaultPlatformCapabilities(): PlatformCapabilities {
  const os = browserOs();
  const isMac = os === 'macos';
  const isWindows = os === 'windows';
  return {
    os,
    localBackend: {
      supported: isMac,
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

let cached: Promise<PlatformCapabilities> | null = null;

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

export function resetPlatformCapabilitiesCache(): void {
  cached = null;
}
