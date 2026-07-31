import { pending, type PendingUploadMeta } from '../tauri';
import { useMeetingApi } from './useMeetingApi';
import { reportUploadFailure } from './useDiagnostics';

/** Merge a chronological list of pending uploads (all sharing one `meetingId`,
 *  as produced by {@link groupByMeetingId}) into one recording's meta: earliest
 *  start, latest end, summed duration. The group's `meetingId` is carried
 *  through so the retry re-attaches to the same meeting instead of orphaning it;
 *  it is omitted when the group has none (an ad-hoc recording → fresh meeting). */
export function mergedMeta(items: PendingUploadMeta[]): {
  startAt: string;
  endAt: string;
  durationSeconds: number;
  meetingId?: number;
} {
  const first = items[0];
  const last = items[items.length - 1];
  const meta = {
    startAt: first.startAt ?? first.createdAt,
    endAt: last.endAt,
    durationSeconds: items.reduce((sum, i) => sum + i.durationSeconds, 0),
  };
  return first.meetingId != null ? { ...meta, meetingId: first.meetingId } : meta;
}

/** Partition pending items by `meetingId` so each server meeting keeps its own
 *  audio. Items that were tied to the same meeting combine into one upload for
 *  that meeting; items with no `meetingId` form a single group uploaded as one
 *  fresh meeting. Chronological order is preserved within and across groups. */
export function groupByMeetingId(items: PendingUploadMeta[]): PendingUploadMeta[][] {
  const groups = new Map<number | null, PendingUploadMeta[]>();
  for (const item of items) {
    const key = item.meetingId ?? null;
    const group = groups.get(key);
    if (group) group.push(item);
    else groups.set(key, [item]);
  }
  return [...groups.values()];
}

/** Concatenate one meetingId group server-side, upload it (to its existing
 *  meeting when it has an id, else a fresh one), then discard its buffers. */
async function uploadGroup(group: PendingUploadMeta[]): Promise<void> {
  const keys = group.map((i) => i.createdAt);
  const meta = mergedMeta(group);

  // Report both legs of the retry separately: a failing `combine` (Rust-side
  // concatenation, e.g. a missing or oversized buffer) is a different bug from
  // a failing upload, and issue #260's "retry does nothing" symptom can't be
  // told apart without knowing which one broke.
  let buf: ArrayBuffer;
  try {
    buf = await pending.combine(keys);
  } catch (e) {
    await reportUploadFailure(e, {
      attempt: 'retry',
      stage: 'combine',
      itemCount: group.length,
      durationSeconds: meta.durationSeconds,
    });
    throw e;
  }

  const blob = new Blob([buf], { type: 'audio/mpeg' });
  try {
    await useMeetingApi().uploadAudio(blob, meta);
  } catch (e) {
    await reportUploadFailure(e, {
      attempt: 'retry',
      bytes: blob.size,
      durationSeconds: meta.durationSeconds,
      itemCount: group.length,
      hasMeetingId: meta.meetingId != null,
    });
    throw e;
  }
  // Upload succeeded; a buffer discard failure must not bubble up — otherwise
  // the caller would retry and double-upload. Match the per-recording path in
  // useBackend.ts/finalizeRecording.
  const cleanup = await Promise.allSettled(keys.map((k) => pending.discardAudio(k)));
  const failed = cleanup.filter((r) => r.status === 'rejected').length;
  if (failed > 0) {
    console.error(`Uploaded combined audio, but failed to discard ${failed} buffered item(s)`);
  }
}

/** Resume pending uploads: group by meeting so each meeting's audio re-attaches
 *  to that meeting (preserving its id), then upload each group. A group that
 *  fails is left buffered for a later retry while the others still upload and
 *  discard; the first failure is re-thrown so the caller surfaces an error.
 *  `items` must be chronological (as `pending.list()` returns). */
export async function combineAndUpload(items: PendingUploadMeta[]): Promise<void> {
  if (items.length === 0) return;
  const results = await Promise.allSettled(groupByMeetingId(items).map(uploadGroup));
  const firstFailure = results.find((r) => r.status === 'rejected');
  if (firstFailure) throw (firstFailure as PromiseRejectedResult).reason;
}

/** Discard every pending upload (the "Discard all" action). */
export async function discardAll(items: PendingUploadMeta[]): Promise<void> {
  await Promise.all(items.map((i) => pending.discardAudio(i.createdAt)));
}
