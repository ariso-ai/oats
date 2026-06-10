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
