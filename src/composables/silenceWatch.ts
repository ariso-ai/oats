/** Universal silence backstop: 15 minutes of no captured sound ends any
 *  recording (manual or auto). Pure so it can be unit-tested with explicit
 *  timestamps. */
export const SILENCE_TIMEOUT_MS = 15 * 60_000;

/**
 * Whether a recording should auto-stop due to silence.
 *
 * Frozen while paused — a deliberately paused recording is never auto-stopped,
 * and (because the caller resets `lastSoundAt` on resume) a paused gap never
 * counts toward the timeout.
 */
export function shouldAutoStop(
  lastSoundAt: number,
  now: number,
  paused: boolean,
  timeoutMs: number = SILENCE_TIMEOUT_MS,
): boolean {
  if (paused) return false;
  return now - lastSoundAt >= timeoutMs;
}

/** Silence prompt: after 10 min of no captured sound, prompt the user to keep
 *  or stop the recording instead of silently ending it. Pure for unit testing. */
export const SILENCE_PROMPT_MS = 10 * 60_000;

/** Grace window after the prompt shows before the recording auto-stops. */
export const SILENCE_GRACE_MS = 60_000;

/**
 * Whether the silence prompt should be shown now.
 * Frozen while paused (a deliberately paused recording is never prompted).
 */
export function shouldPromptSilence(
  lastSoundAt: number,
  now: number,
  paused: boolean,
): boolean {
  if (paused) return false;
  return now - lastSoundAt >= SILENCE_PROMPT_MS;
}

/**
 * Whether a prompted recording should now auto-stop. Returns false — i.e. the
 * pending stop is cancelled — if paused, or if audio resumed after the prompt
 * was shown (`lastSoundAt` advanced past `promptShownAt`).
 */
export function shouldAutoStopAfterPrompt(
  promptShownAt: number,
  lastSoundAt: number,
  now: number,
  paused: boolean,
): boolean {
  if (paused) return false;
  if (lastSoundAt > promptShownAt) return false;
  return now - promptShownAt >= SILENCE_GRACE_MS;
}
