import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import { deriveEnabledFromLegacy, type RecordingEnabled } from '../views/recordingSettings';

const SETTINGS_PATH = 'settings.json';
const MIC_KEY = 'recordMicEnabled';
const SYS_KEY = 'recordSystemAudioEnabled';
const LEGACY_KEY = 'recordingMode';

function isMac(): boolean {
  return typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');
}

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

/** Prompt for / verify macOS Screen Recording permission (system audio). */
export async function ensureSystemAudioPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('request_screen_capture_permission');
  } catch {
    return false;
  }
}

/** Current Screen Recording status without prompting (for initial UI state). */
export async function checkSystemAudioPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('check_screen_capture_permission');
  } catch {
    return false;
  }
}

/** Open System Settings → Privacy → Microphone. macOS only; no-op elsewhere. */
export async function openMicSettings(): Promise<void> {
  if (!isMac()) return;
  await openUrl('x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone');
}

/** Open System Settings → Privacy → Screen Recording. macOS only; no-op elsewhere. */
export async function openSystemAudioSettings(): Promise<void> {
  if (!isMac()) return;
  await openUrl('x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture');
}
