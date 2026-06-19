import type { BackendId } from './useBackend';
import type { ScheduledMeeting } from './useMeetingApi';
import { pickDefaultMeeting } from './pickDefaultMeeting';

export interface Association {
  /** 'matched' → attach to a current calendar meeting; 'confirm' → ask the user. */
  kind: 'matched' | 'confirm';
  meetingId?: number;
}

/**
 * Decide how an auto-triggered recording associates. Only the Ariso backend has
 * a calendar; a meeting "happening now" (per `pickDefaultMeeting`) is matched,
 * otherwise — and always for Local — we fall back to a user confirmation.
 */
export function resolveAssociation(
  backendId: BackendId,
  meetings: ScheduledMeeting[],
  now: Date,
): Association {
  if (backendId !== 'ariso') return { kind: 'confirm' };
  const picked = pickDefaultMeeting(meetings, now);
  if (picked.kind === 'current' && picked.featured) {
    return { kind: 'matched', meetingId: picked.featured.id };
  }
  return { kind: 'confirm' };
}
