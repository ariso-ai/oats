import { getPlatformCapabilities, type PlatformCapabilities } from '../tauri';

/** Non-Tauri rendering is deliberately unsupported. Browser previews can still
 * render, but only the native capability command may enable privileged paths. */
export function defaultPlatformCapabilities(): PlatformCapabilities {
  return {
    os: 'linux',
    localBackend: { supported: false, engine: null },
    systemAudio: { supported: false, settingsUrl: null },
    autoRecord: { supported: false },
    nativeShare: { supported: false },
    notificationSettingsUrl: null,
    microphoneSettingsUrl: null,
  };
}

function hasTauriBridge(): boolean {
  return '__TAURI_INTERNALS__' in globalThis;
}

// A capability snapshot is immutable for the lifetime of one app binary, so one
// promise per webview avoids repeated IPC while still sharing an in-flight call.
// Windows in separate webviews have their own cache; no cross-window state is
// required because every native response is derived from the same build.
let cached: Promise<PlatformCapabilities> | null = null;

/** Uses browser defaults only outside Tauri. A packaged-app IPC failure remains
 * an error so callers can surface the broken native integration and retry. */
export function loadPlatformCapabilities(): Promise<PlatformCapabilities> {
  if (!hasTauriBridge()) return Promise.resolve(defaultPlatformCapabilities());
  if (!cached) {
    cached = Promise.resolve().then(() => getPlatformCapabilities());
  }
  return cached;
}

/** Exists for tests and development reloads that need a fresh IPC attempt. App
 * code should treat capabilities as process-static rather than mutable state. */
export function resetPlatformCapabilitiesCache(): void {
  cached = null;
}
