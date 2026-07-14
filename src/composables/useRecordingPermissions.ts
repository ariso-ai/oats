import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import {
  deriveEnabledFromLegacy,
  type RecordingEnabled,
} from '../views/recordingSettings';
import { loadPlatformCapabilities } from './usePlatformCapabilities';

const SETTINGS_PATH = 'settings.json';
const MIC_KEY = 'recordMicEnabled';
const SYS_KEY = 'recordSystemAudioEnabled';
const LEGACY_KEY = 'recordingMode';

/**
 * Load both recording-source flags, migrating from the legacy `recordingMode`
 * key on first run. Native capabilities determine whether system audio may
 * remain enabled; the loader never substitutes a different source.
 */
export async function loadRecordingEnabled(): Promise<RecordingEnabled> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const capabilities = await loadPlatformCapabilities();
  const mic = await store.get<boolean>(MIC_KEY);
  const sys = await store.get<boolean>(SYS_KEY);
  let persisted: RecordingEnabled;

  if (typeof mic === 'boolean' && typeof sys === 'boolean') {
    persisted = { mic, systemAudio: sys };
  } else {
    const legacy = await store.get<string>(LEGACY_KEY);
    persisted = legacy
      ? deriveEnabledFromLegacy(legacy)
      : { mic: true, systemAudio: capabilities.systemAudio.supported };
  }
  persisted = {
    mic: typeof mic === 'boolean' ? mic : persisted.mic,
    systemAudio:
      capabilities.systemAudio.supported &&
      (typeof sys === 'boolean' ? sys : persisted.systemAudio),
  };

  // Initialize missing keys and clear a source this binary cannot capture.
  // Microphone state is never changed as a substitute for system audio.
  if (typeof mic !== 'boolean') await store.set(MIC_KEY, persisted.mic);
  if (typeof sys !== 'boolean' || sys !== persisted.systemAudio) {
    await store.set(SYS_KEY, persisted.systemAudio);
  }
  return persisted;
}

export async function setMicEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(MIC_KEY, enabled);
}

export async function setSystemAudioEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(SYS_KEY, enabled);
}

/** Prompt for / verify microphone permission by opening and closing a stream. */
export async function ensureMicPermission(): Promise<boolean> {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    stream.getTracks().forEach((t) => t.stop());
    return true;
  } catch {
    return false;
  }
}

/**
 * Prompt for / verify the macOS "System Audio Recording" permission. System
 * audio is captured via Core Audio process taps (macOS 14.4+), so the OS lists
 * the app under the "System Audio Recording Only" section of Privacy &
 * Security → Screen & System Audio Recording — not full screen recording.
 */
export async function ensureSystemAudioPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('request_screen_capture_permission');
  } catch {
    return false;
  }
}

/** Current system-audio recording status (best-effort; there is no public
 * preflight API, so this attempts a throwaway tap). */
export async function checkSystemAudioPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('check_screen_capture_permission');
  } catch {
    return false;
  }
}

/** Open the OS microphone permission pane when the platform exposes one. The
 * native capability layer owns the exact allowlisted URL; this helper remains a
 * UI action and does not infer whether permission is currently denied. */
export async function openMicSettings(): Promise<void> {
  const url = (await loadPlatformCapabilities()).microphoneSettingsUrl;
  if (url) await openUrl(url);
}

/**
 * Open the OS system-audio settings pane when the platform exposes one. On
 * macOS this is Privacy & Security; on Windows this is the Sound settings page.
 * A URL can exist even when capture is unsupported, so callers must still use
 * `systemAudio.supported` for feature gating.
 */
export async function openSystemAudioSettings(): Promise<void> {
  const url = (await loadPlatformCapabilities()).systemAudio.settingsUrl;
  if (url) await openUrl(url);
}
