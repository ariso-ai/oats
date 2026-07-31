/**
 * Whether an ariso meeting `status` means the meeting was canceled — the state
 * a deleted/canceled calendar event lands in (the backend normalizes calendar
 * deletions into `status: 'cancelled'`). The web app keys off the exact
 * `'cancelled'` spelling; we also accept the American spelling and stray
 * case/whitespace so a payload variation can't silently show a canceled
 * meeting as a live one.
 */
export function isCanceledMeetingStatus(status: unknown): boolean {
  if (typeof status !== 'string') return false;
  const s = status.trim().toLowerCase();
  return s === 'cancelled' || s === 'canceled';
}

function statusIs(status: unknown, want: string): boolean {
  return typeof status === 'string' && status.trim().toLowerCase() === want;
}

/** Ari (the notetaker bot) is in the meeting right now. */
export function isJoinedMeetingStatus(status: unknown): boolean {
  return statusIs(status, 'joined');
}

/** The meeting is over as far as the backend is concerned. */
export function isDoneMeetingStatus(status: unknown): boolean {
  return statusIs(status, 'done');
}

/** Which Ari chip a meeting should show: none, the scheduled promise, or the
 *  present-tense "Ari has joined". */
export type AriJoinChip = 'will-join' | 'joined' | null;

export interface AriJoinChipInput {
  /** Ari is scheduled to auto-join and record this meeting server-side. */
  autoJoinScheduled?: boolean;
  /** Ariso lifecycle status ('created' | 'joined' | 'done' | 'cancelled' | …). */
  status?: string | null;
  startAt?: string | null;
  endAt?: string | null;
}

function toMs(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const t = new Date(iso).getTime();
  return Number.isNaN(t) ? null : t;
}

/**
 * Decide the Ari chip for a meeting.
 *
 * Once the meeting is over — its end time has passed, or the backend moved it
 * to `done` — the chip disappears entirely: a promise about the future reads as
 * wrong on a finished meeting. While the meeting is running and Ari has
 * actually joined, the chip switches to the present tense. Everything else
 * (canceled meetings, meetings with no auto-join) is handled by the callers'
 * own chips or shows the plain "Ari will join".
 *
 * A meeting with no parsable end time is never treated as ended — calendar
 * meetings always carry one, and guessing would hide the chip mid-meeting.
 */
export function ariJoinChip(meeting: AriJoinChipInput, now: Date): AriJoinChip {
  if (!meeting.autoJoinScheduled) return null;
  if (isCanceledMeetingStatus(meeting.status)) return null;
  if (isDoneMeetingStatus(meeting.status)) return null;

  const t = now.getTime();
  const end = toMs(meeting.endAt);
  if (end !== null && end <= t) return null;

  const start = toMs(meeting.startAt);
  const started = start !== null && start <= t;
  if (started && isJoinedMeetingStatus(meeting.status)) return 'joined';
  return 'will-join';
}
