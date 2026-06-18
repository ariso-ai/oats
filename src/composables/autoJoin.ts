import type { BackendId } from './useBackend';

/**
 * Lenient truthiness for the ariso `auto_join_scheduled` flag, which may arrive
 * as a bool, a number, or a string. Mirrors the Rust `truthy()` in
 * `src-tauri/src/meeting_notifications.rs` so the desktop and backend agree.
 */
export function arisoTruthy(v: unknown): boolean {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  if (typeof v === 'string') return v === 'true' || v === '1';
  return false;
}

/**
 * Whether starting a recording for this meeting should first confirm with the
 * user. Only ariso meetings flagged for Ari's server-side auto-join qualify —
 * a local recording of them would be redundant.
 */
export function shouldConfirmAriJoin(
  backendId: BackendId,
  autoJoinScheduled: boolean | undefined,
): boolean {
  return backendId === 'ariso' && autoJoinScheduled === true;
}
