/**
 * Meeting-end stop prompt: when a recording attached to a calendar meeting runs
 * past that meeting's scheduled end — or the next calendar meeting has already
 * started — prompt the user to keep or stop instead of silently bleeding into
 * the next back-to-back call. Pure for unit testing; mirrors silenceWatch.ts.
 */

/** Grace after the scheduled end before the first prompt. */
export const MEETING_END_GRACE_MS = 2 * 60_000;
/** How long the card stays up before the watch returns to idle (= keep). Must
 *  match the Rust MEETING_END_PROMPT_SECONDS cosmetic countdown. */
export const MEETING_END_PROMPT_TIMEOUT_MS = 30_000;
/** Delay between the first prompt and the single re-prompt. */
export const MEETING_END_REPROMPT_MS = 5 * 60_000;
/** Total prompts per recording (initial + one re-prompt). */
export const MEETING_END_MAX_PROMPTS = 2;

/**
 * Whether the meeting-end prompt should be shown on this tick. Frozen while
 * paused, disabled when there's neither a scheduled end (`endAt === null`) nor
 * a known next-meeting start (`nextStartAt === null`), and capped at
 * MEETING_END_MAX_PROMPTS. The first prompt fires at `endAt + grace` OR the
 * moment the next calendar meeting starts, whichever comes first — the next
 * meeting's start is the transition point, so back-to-back (and slightly
 * overlapping) meetings prompt without waiting out the grace. The second
 * prompt only after `lastPromptAt + reprompt`.
 */
export function shouldPromptMeetingEnd(
  endAt: number | null,
  now: number,
  paused: boolean,
  promptsShown: number,
  lastPromptAt: number | null,
  nextStartAt: number | null = null,
): boolean {
  if (paused || (endAt === null && nextStartAt === null)) return false;
  if (promptsShown >= MEETING_END_MAX_PROMPTS) return false;
  if (promptsShown === 0) {
    return (
      (endAt !== null && now >= endAt + MEETING_END_GRACE_MS) ||
      (nextStartAt !== null && now >= nextStartAt)
    );
  }
  return lastPromptAt !== null && now >= lastPromptAt + MEETING_END_REPROMPT_MS;
}

export interface MeetingEndInfo {
  /** Scheduled end as epoch ms, or null when absent / unparseable. */
  endAt: number | null;
  /** Meeting title (for the card subtitle), or null. */
  title: string | null;
}

/**
 * Pull the attached meeting's scheduled end + title out of the scheduled-meetings
 * list (the shape that carries `end_at`, unlike `getMeeting`). Returns null endAt
 * when the meeting isn't found, has no `end_at`, or it doesn't parse.
 */
export function findMeetingEndAt(
  meetings: ReadonlyArray<{ id: number; end_at?: string | null; title: string | null }>,
  meetingId: number,
): MeetingEndInfo {
  const m = meetings.find((x) => x.id === meetingId);
  if (!m) return { endAt: null, title: null };
  if (!m.end_at) return { endAt: null, title: m.title ?? null };
  const ms = new Date(m.end_at).getTime();
  return { endAt: Number.isFinite(ms) ? ms : null, title: m.title ?? null };
}

export interface NextMeetingInfo {
  /** Next meeting's scheduled start as epoch ms, or null when there is none. */
  startAt: number | null;
  /** Next meeting's title (for the prompt copy), or null. */
  title: string | null;
}

/**
 * The earliest meeting starting strictly AFTER the attached meeting's start —
 * the transition point for back-to-back and slightly overlapping calls.
 * Meetings sharing the attached start are parallel, not "next", so they're
 * skipped. Returns nulls when the attached meeting is missing or has no
 * parseable start (the watch then degrades to the end-time trigger alone).
 */
export function findNextMeetingStart(
  meetings: ReadonlyArray<{ id: number; start_at?: string | null; title: string | null }>,
  meetingId: number,
): NextMeetingInfo {
  const current = meetings.find((x) => x.id === meetingId);
  const currentStart = current?.start_at ? new Date(current.start_at).getTime() : NaN;
  if (!Number.isFinite(currentStart)) return { startAt: null, title: null };
  let next: NextMeetingInfo = { startAt: null, title: null };
  for (const m of meetings) {
    if (m.id === meetingId || !m.start_at) continue;
    const ms = new Date(m.start_at).getTime();
    if (!Number.isFinite(ms) || ms <= currentStart) continue;
    if (next.startAt === null || ms < next.startAt) next = { startAt: ms, title: m.title ?? null };
  }
  return next;
}
