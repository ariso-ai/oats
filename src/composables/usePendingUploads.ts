import { pending, type PendingUploadMeta } from '../tauri';
import { useMeetingApi } from './useMeetingApi';

/** Merge a chronological list of pending uploads into one recording's meta:
 *  earliest start, latest end, summed duration. `meetingId` is intentionally
 *  dropped so the combined upload always creates one fresh meeting. */
export function mergedMeta(items: PendingUploadMeta[]): {
  startAt: string;
  endAt: string;
  durationSeconds: number;
} {
  const first = items[0];
  const last = items[items.length - 1];
  return {
    startAt: first.startAt ?? first.createdAt,
    endAt: last.endAt,
    durationSeconds: items.reduce((sum, i) => sum + i.durationSeconds, 0),
  };
}

/** Concatenate all pending uploads server-side, upload as one meeting, then
 *  discard the combined buffers. Leaves everything in place if the upload
 *  fails. `items` must be chronological (as `pending.list()` returns). */
export async function combineAndUpload(items: PendingUploadMeta[]): Promise<void> {
  if (items.length === 0) return;
  const keys = items.map((i) => i.createdAt);
  const buf = await pending.combine(keys);
  const blob = new Blob([buf], { type: 'audio/mpeg' });
  await useMeetingApi().uploadAudio(blob, mergedMeta(items));
  await Promise.all(keys.map((k) => pending.discardAudio(k)));
}

/** Discard every pending upload (the "Discard all" action). */
export async function discardAll(items: PendingUploadMeta[]): Promise<void> {
  await Promise.all(items.map((i) => pending.discardAudio(i.createdAt)));
}
