import {
  isPermissionGranted,
  requestPermission,
} from '@tauri-apps/plugin-notification';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import { emit } from '@tauri-apps/api/event';
import { loadPlatformCapabilities } from './usePlatformCapabilities';

// The Pusher connection + notification dispatch live natively in Rust
// (see src-tauri/src/meeting_notifications.rs) because macOS suspends hidden
// webviews. This module only owns the settings toggle and OS permission UI,
// and broadcasts SYNC_EVENT so the native orchestrator re-evaluates its state.

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'meetingNotificationsEnabled';
export const SYNC_EVENT = 'meeting-notifications-sync';

/** Whether meeting notifications are enabled (defaults to true). */
export async function isMeetingNotificationsEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const value = await store.get<boolean>(ENABLED_KEY);
  return value !== false;
}

/** Persist the enabled flag and broadcast a sync so the orchestrator reacts. */
export async function setMeetingNotificationsEnabled(
  enabled: boolean
): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(ENABLED_KEY, enabled);
  await emit(SYNC_EVENT);
}

/** Broadcast a sync (used after sign-in / sign-out). */
export async function emitNotificationsSync(): Promise<void> {
  await emit(SYNC_EVENT);
}

/**
 * Ensure the OS notification permission is granted, prompting the user if it
 * has not yet been decided. Called when the settings toggle is switched on.
 */
export async function ensureNotificationPermission(): Promise<boolean> {
  let granted = await isPermissionGranted();
  if (!granted) {
    granted = (await requestPermission()) === 'granted';
  }
  return granted;
}

/**
 * Open the OS notification settings so the user can grant permission manually.
 * Used when a permission request can't surface a prompt.
 */
export async function openNotificationSettings(): Promise<void> {
  const url = (await loadPlatformCapabilities()).notificationSettingsUrl;
  if (url) await openUrl(url);
}
