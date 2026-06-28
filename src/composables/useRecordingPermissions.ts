import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import { deriveEnabledFromLegacy, type RecordingEnabled } from '../views/recordingSettings';
import { loadPlatformCapabilities } from './usePlatformCapabilities';

const SETTINGS_PATH = 'settings.json';
const MIC_KEY = 'recordMicEnabled';
const SYS_KEY = 'recordSystemAudioEnabled';
const LEGACY_KEY = 'recordingMode';

/**
 * Load both recording-source flags, migrating from the legacy `recordingMode`
 * key on first run (absent → defaults to both on). The migrated values are
 * written back so the legacy key is never read again.
 */
export async function loadRecordingEnabled(): Promise<RecordingEnabled> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const mic = await store.get<boolean>(MIC_KEY);
  const sys = await store.get<boolean>(SYS_KEY);
  if (typeof mic === 'boolean' && typeof sys === 'boolean') {
    return { mic, systemAudio: sys };
  }
  // At least one new key is missing — migrate the missing one(s) from the
  // legacy `recordingMode` while preserving any new key already written.
  const derived = deriveEnabledFromLegacy(await store.get<string>(LEGACY_KEY));
  const result: RecordingEnabled = {
    mic: typeof mic === 'boolean' ? mic : derived.mic,
    systemAudio: typeof sys === 'boolean' ? sys : derived.systemAudio,
  };
  if (typeof mic !== 'boolean') await store.set(MIC_KEY, result.mic);
  if (typeof sys !== 'boolean') await store.set(SYS_KEY, result.systemAudio);
  return result;
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

/** Open the OS microphone permission pane when the platform exposes one. */
export async function openMicSettings(): Promise<void> {
  const url = (await loadPlatformCapabilities()).microphoneSettingsUrl;
  if (url) await openUrl(url);
}

/**
 * Open the OS system-audio settings pane when the platform exposes one. On
 * macOS this is Privacy & Security; on Windows this is the Sound settings page.
 */
export async function openSystemAudioSettings(): Promise<void> {
  const url = (await loadPlatformCapabilities()).systemAudio.settingsUrl;
  if (url) await openUrl(url);
}
