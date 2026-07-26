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
