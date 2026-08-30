/** Characters that are illegal or troublesome in a file name on macOS and
 *  Windows. A meeting title is free-form user text, so it can contain any
 *  of them. */
const ILLEGAL_FILENAME_CHARS = /[/\\:*?"<>|]/g;

/** Turn a meeting title into a safe file-name stem. Falls back to "Meeting"
 *  when the title is blank or sanitizes away to nothing. */
export function sanitizeFilename(title: string | undefined): string {
  const cleaned = (title ?? '')
    .replace(ILLEGAL_FILENAME_CHARS, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return cleaned || 'Meeting';
}

/** Format an ISO timestamp as `YYYY-MM-DD` in the user's local time (the date
 *  they'd say the meeting happened on). Returns '' when the timestamp is
 *  missing or unparseable so callers can drop the date entirely. */
export function isoDateStamp(startAt: string | undefined): string {
  if (!startAt) return '';
  const d = new Date(startAt);
  if (Number.isNaN(d.getTime())) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Default name offered in the save dialog: `{title} transcript - {date}.md`.
 *  The user can still rename it in the dialog. */
export function transcriptFilename(
  title: string | undefined,
  startAt: string | undefined
): string {
  const stem = `${sanitizeFilename(title)} transcript`;
  const stamp = isoDateStamp(startAt);
  return stamp ? `${stem} - ${stamp}.md` : `${stem}.md`;
}
