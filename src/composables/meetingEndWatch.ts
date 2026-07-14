/**
 * Meeting-end stop prompt: when a recording attached to a calendar meeting runs
 * past that meeting's scheduled end, prompt the user to keep or stop instead of
 * silently bleeding into the next back-to-back call. Pure for unit testing;
 * mirrors silenceWatch.ts.
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
 * paused, disabled when there's no scheduled end (`endAt === null`), and capped
 * at MEETING_END_MAX_PROMPTS. The first prompt fires at `endAt + grace`; the
 * second only after `lastPromptAt + reprompt`.
 */
export function shouldPromptMeetingEnd(
  endAt: number | null,
  now: number,
  paused: boolean,
  promptsShown: number,
  lastPromptAt: number | null,
): boolean {
  if (paused || endAt === null) return false;
  if (promptsShown >= MEETING_END_MAX_PROMPTS) return false;
  if (promptsShown === 0) return now >= endAt + MEETING_END_GRACE_MS;
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
