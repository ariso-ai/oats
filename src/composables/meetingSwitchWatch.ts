/**
 * Meeting-switch prompt: when a recording attached to a calendar meeting is
 * still running as the NEXT calendar meeting starts, prompt the user to switch
 * (finalize the current recording in the background and immediately record the
 * new meeting) instead of silently bleeding into the next back-to-back call.
 * Pure for unit testing; mirrors silenceWatch.ts.
 */

/** How long the card stays up before it dismisses (= keep recording). Must
 *  match the Rust MEETING_SWITCH_PROMPT_SECONDS cosmetic countdown. */
export const MEETING_SWITCH_PROMPT_TIMEOUT_MS = 30_000;

export interface NextMeeting {
  /** The next meeting's id — the attach target when the user accepts. */
  id: number;
  /** Scheduled start as epoch ms. */
  startAt: number;
  /** Title (for the card subtitle), or null. */
  title: string | null;
}

/**
 * Whether the switch prompt should be shown on this tick. Frozen while paused
 * and shown at most once per candidate next meeting (`alreadyPrompted`). The
 * next meeting's start is the transition point — no grace: back-to-back (and
 * slightly overlapping) meetings prompt the moment the next one starts.
 */
export function shouldPromptMeetingSwitch(
  nextStartAt: number,
  now: number,
  paused: boolean,
  alreadyPrompted: boolean,
): boolean {
  if (paused || alreadyPrompted) return false;
  return now >= nextStartAt;
}

/**
 * The earliest meeting starting strictly AFTER the attached meeting's start —
 * the transition point for back-to-back and slightly overlapping calls.
 * Meetings sharing the attached start are parallel, not "next", so they're
 * skipped. Returns null when the attached meeting is missing, has no parseable
 * start, or nothing starts after it.
 */
export function findNextMeeting(
  meetings: ReadonlyArray<{ id: number; start_at?: string | null; title: string | null }>,
  meetingId: number,
): NextMeeting | null {
  const current = meetings.find((x) => x.id === meetingId);
  const currentStart = current?.start_at ? new Date(current.start_at).getTime() : NaN;
  if (!Number.isFinite(currentStart)) return null;
  let next: NextMeeting | null = null;
  for (const m of meetings) {
    if (m.id === meetingId || !m.start_at) continue;
    const ms = new Date(m.start_at).getTime();
    if (!Number.isFinite(ms) || ms <= currentStart) continue;
    if (next === null || ms < next.startAt) next = { id: m.id, startAt: ms, title: m.title ?? null };
  }
  return next;
}
