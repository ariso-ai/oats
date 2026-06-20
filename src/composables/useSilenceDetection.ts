import { load } from '@tauri-apps/plugin-store';

// Gates the silence-detection prompt (WaveformView's 1s silenceTimer loop). Read
// once when a recording window mounts — toggling mid-recording only affects the
// next recording, mirroring how the mic/system-audio source flags are read on
// mount. No native monitor reacts to this, so there's no sync broadcast.

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'silenceDetectionEnabled';

/** Whether silence detection is enabled (defaults to true). */
export async function isSilenceDetectionEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const value = await store.get<boolean>(ENABLED_KEY);
  return value !== false;
}

/** Persist the enabled flag. */
export async function setSilenceDetectionEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(ENABLED_KEY, enabled);
}
