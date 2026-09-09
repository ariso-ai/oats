import { ref, type Ref } from 'vue';
import { useMeetingApi, isMeetingNotesReady } from './useMeetingApi';

/** How often to re-ask the server whether an uploaded meeting has content yet.
 *  Heavier than the local pipeline's on-disk 2s check and far less
 *  latency-sensitive: this is a background confirmation, not a progress bar. */
const POLL_MS = 5000;

export interface MeetingProcessing {
  /** Start treating `meetingId` as "uploaded, still processing". */
  markUploaded: (meetingId: string | number) => void;
  /** Reactive membership check — read it inside a computed/template. */
  isProcessing: (meetingId: string | number) => boolean;
  /** Bumps every time a meeting leaves the tracked set (its content landed),
   *  so callers can reload their list without re-deriving membership per tick. */
  version: Ref<number>;
  /** Drop all tracking. Used when the active backend changes (the ids belong to
   *  Ariso) and by tests, which share this module-scoped state. */
  reset: () => void;
}

// Module-scoped on purpose: the Library row and the detail panel must agree on
// which meetings are processing, and the poll should run once for both. State
// is session-only — nothing is persisted, so an app restart forgets everything
// (see the spec's Non-goals).
const tracked = ref<Set<string>>(new Set());
const version = ref(0);
let timer: ReturnType<typeof setTimeout> | null = null;
// Invalidates in-flight polls across a reset so a late response can't resurrect
// or drop an id that belongs to a newer generation of the tracked set.
let token = 0;

function clearTimer(): void {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}

function schedule(my: number): void {
  clearTimer();
  timer = setTimeout(() => void poll(my), POLL_MS);
}

async function poll(my: number): Promise<void> {
  if (my !== token || tracked.value.size === 0) return;
  const api = useMeetingApi();
  const ids = [...tracked.value];
  // Each id resolves independently: one meeting's failed request must not stop
  // the others from being confirmed on this tick.
  const results = await Promise.all(
    ids.map(async (id) => {
      try {
        return isMeetingNotesReady(await api.getMeetingNotes(id));
      } catch (e) {
        // A transient network blip keeps the meeting tracked — better to show
        // "Processing…" a little longer than to flip back to the ambiguous
        // empty state. Mirrors useLocalRecordingProgress's retry behavior.
        console.error('meeting processing poll failed', e);
        return false;
      }
    })
  );
  if (my !== token) return;
  const done = ids.filter((_, i) => results[i]);
  if (done.length) {
    const next = new Set(tracked.value);
    for (const id of done) next.delete(id);
    tracked.value = next;
    version.value++;
  }
  if (tracked.value.size > 0) schedule(my);
  else clearTimer();
}

function markUploaded(meetingId: string | number): void {
  const id = String(meetingId);
  if (!id || tracked.value.has(id)) return;
  const next = new Set(tracked.value);
  next.add(id);
  tracked.value = next;
  if (!timer) schedule(token);
}

function isProcessing(meetingId: string | number): boolean {
  return tracked.value.has(String(meetingId));
}

function reset(): void {
  token++;
  clearTimer();
  tracked.value = new Set();
}

/**
 * Tracks Ariso meetings this session just uploaded and polls until the server
 * has produced a transcript or notes for them.
 *
 * Ariso exposes no "processing" status for audio-uploaded meetings — its
 * `status` field only carries the calendar-call lifecycle — so "processing" is
 * inferred client-side from content absence for meetings we know we uploaded.
 * Local recordings need no equivalent: their state already lives on disk
 * (`useLocalRecordingProgress`).
 */
export function useMeetingProcessing(): MeetingProcessing {
  return { markUploaded, isProcessing, version, reset };
}
