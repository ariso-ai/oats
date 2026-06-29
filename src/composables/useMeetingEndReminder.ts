import { load } from '@tauri-apps/plugin-store';

// Gates the meeting-stop reminder prompt (WaveformView's 1s meetingEndTimer loop
// that fires once the attached calendar meeting's scheduled end has passed). Read
// once when a recording window mounts — toggling mid-recording only affects the
// next recording, mirroring useSilenceDetection. No native monitor reacts to
// this, so there's no sync broadcast.

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'meetingEndReminderEnabled';

/** Whether the meeting-stop reminder is enabled (defaults to true). */
export async function isMeetingEndReminderEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const value = await store.get<boolean>(ENABLED_KEY);
  return value !== false;
}

/** Persist the enabled flag. */
export async function setMeetingEndReminderEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(ENABLED_KEY, enabled);
}
