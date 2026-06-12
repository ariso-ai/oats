/** The id a local recording will be stored under is deterministic: the Rust
 *  side derives it from the recording's start timestamp (`sanitize_iso_to_id`
 *  in storage.rs). Mirroring that mapping lets the library address the
 *  recording — red dot, selection, the embedded recorder strip — while the
 *  capture is still running, and have that identity survive finalize. */

/** TS mirror of Rust `sanitize_iso_to_id`: drop a trailing `Z` or
 *  `±HH:MM` offset and sub-second precision, replace `:` with `-`, and
 *  re-append `Z`. `2026-06-02T14:30:05.123Z` → `2026-06-02T14-30-05Z`. */
export function localRecordingIdFromStart(iso: string): string {
  const noOffset = iso.replace(/([+-]\d{2}:\d{2}|Z)$/, '');
  const dot = noOffset.indexOf('.');
  const head = dot >= 0 ? noOffset.slice(0, dot) : noOffset;
  return `${head.replaceAll(':', '-')}Z`;
}

/** Inverse of `localRecordingIdFromStart` (seconds precision). Returns null
 *  for anything that is not a local recording id — e.g. a numeric Ariso
 *  meeting id — so callers can tell the two apart. */
export function timestampFromLocalRecordingId(id: string): string | null {
  const m = id.match(/^(\d{4}-\d{2}-\d{2}T\d{2})-(\d{2})-(\d{2})Z$/);
  return m ? `${m[1]}:${m[2]}:${m[3]}Z` : null;
}
