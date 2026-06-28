import { invoke } from '@tauri-apps/api/core';
import { load } from '@tauri-apps/plugin-store';
import { emit } from '@tauri-apps/api/event';
import { loadPlatformCapabilities } from './usePlatformCapabilities';

// The mic monitor lives natively in Rust (src-tauri/src/mic_monitor.rs). This
// module owns only the settings toggle + support probe, and broadcasts a sync
// so the native monitor re-evaluates (start/stop) without an app restart.

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'autoRecordEnabled';
export const AUTO_RECORD_SYNC_EVENT = 'auto-record-sync';

/** Whether auto-record is enabled (defaults to true). */
export async function isAutoRecordEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const value = await store.get<boolean>(ENABLED_KEY);
  return value !== false;
}

/** Persist the enabled flag and broadcast a sync so the monitor reacts. */
export async function setAutoRecordEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(ENABLED_KEY, enabled);
  await emit(AUTO_RECORD_SYNC_EVENT);
}

/** Whether the OS supports per-process mic detection (macOS 14.4+). */
export async function isAutoRecordSupported(): Promise<boolean> {
  try {
    const caps = await loadPlatformCapabilities();
    if (!caps.autoRecord.supported) return false;
    return await invoke<boolean>('auto_record_supported');
  } catch {
    return false;
  }
}
