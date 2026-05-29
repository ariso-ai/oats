import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
  onAction,
  type Options,
} from '@tauri-apps/plugin-notification';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import { emit } from '@tauri-apps/api/event';
import { api, auth, getDesktopConfig } from '../tauri';
import { usePusher, type PusherHandle } from './usePusher';
import { listInboxMessages } from './useInbox';
import {
  findInboxMessage,
  buildPrepNotification,
  prepChannelName,
  MEETING_PREP_SOURCE,
} from './notifications';

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'meetingNotificationsEnabled';
export const SYNC_EVENT = 'meeting-notifications-sync';

let handle: PusherHandle | null = null;
let starting = false;
let actionListenerReady = false;

// Maps a notification id to the URL its click should open. Best-effort:
// Tauri v2 desktop click callbacks are unreliable across platforms.
const urlById = new Map<number, string>();
let nextNotificationId = 1;

interface PrepCompleteEvent {
  meetingPrepId: number;
  eventId?: number;
}

interface CurrentUser {
  org_id: number;
  id: number;
}

/** Whether meeting notifications are enabled (defaults to true). */
export async function isMeetingNotificationsEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  const value = await store.get<boolean>(ENABLED_KEY);
  return value !== false;
}

/** Persist the enabled flag and broadcast a sync so the main window reacts. */
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

async function ensurePermission(): Promise<boolean> {
  let granted = await isPermissionGranted();
  if (!granted) {
    granted = (await requestPermission()) === 'granted';
  }
  return granted;
}

async function ensureActionListener(): Promise<void> {
  if (actionListenerReady) return;
  actionListenerReady = true;
  // Best-effort: open the deep link if the platform delivers a click action.
  await onAction((notification: Options) => {
    const id = notification.id;
    if (id == null) return;
    const url = urlById.get(id);
    if (url) {
      urlById.delete(id);
      void openUrl(url);
    }
  });
}

async function onPrepComplete(
  meetingPrepId: number,
  webAppBaseUrl: string
): Promise<void> {
  try {
    const items = await listInboxMessages(20);
    const found = findInboxMessage(items, MEETING_PREP_SOURCE, meetingPrepId);
    const { title, body, url } = buildPrepNotification(
      found?.message ?? null,
      meetingPrepId,
      webAppBaseUrl
    );
    const id = nextNotificationId++;
    urlById.set(id, url);
    sendNotification({ id, title, body });
  } catch (err) {
    console.error('Failed to handle meeting-prep-complete:', err);
  }
}

/** Connect to Pusher and start surfacing meeting-prep notifications. */
export async function startMeetingNotifications(): Promise<void> {
  if (handle || starting) return;
  starting = true;
  try {
    const session = await auth.checkSession();
    if (!session) return;
    if (!(await isMeetingNotificationsEnabled())) return;
    if (!(await ensurePermission())) return;

    const { webAppBaseUrl } = await getDesktopConfig();

    const meRes = await api.request('GET', '/auth/me');
    if (meRes.status !== 200) return;
    const me = meRes.data as CurrentUser;

    await ensureActionListener();

    const h = await usePusher(prepChannelName(me.org_id, me.id));
    h.channel.bind('meeting-prep-complete', (event: PrepCompleteEvent) => {
      void onPrepComplete(event.meetingPrepId, webAppBaseUrl);
    });
    handle = h;
  } catch (err) {
    console.error('Failed to start meeting notifications:', err);
  } finally {
    starting = false;
  }
}

/** Disconnect from Pusher. */
export async function stopMeetingNotifications(): Promise<void> {
  if (handle) {
    handle.cleanup();
    handle = null;
  }
}
