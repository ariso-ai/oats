import type { MeetingActionItem, MeetingParticipantInfo } from './useBackend';
import type { ActionItemFollowUp } from './useMeetingApi';

/** Somewhere an Ariso action item can be moved to: an attendee's
 *  meeting_participants row, or nobody. */
export interface AssignTarget {
  key: string;
  name: string;
  meetingParticipantId: number | null;
  avatarUrl: string | null;
}

export const UNASSIGNED_TARGET: AssignTarget = {
  key: 'unassigned',
  name: 'Unassigned',
  meetingParticipantId: null,
  avatarUrl: null,
};

/** The attendees an item can be reassigned to, in participant order, plus
 *  Unassigned. Only rows carrying a meeting_participants id can be written to;
 *  calendar-only guests without one are skipped. */
export function buildAssignTargets(participants: MeetingParticipantInfo[]): AssignTarget[] {
  const seen = new Set<number>();
  const targets: AssignTarget[] = [];
  for (const p of participants) {
    const id = p.meetingParticipantId;
    if (typeof id !== 'number' || seen.has(id)) continue;
    seen.add(id);
    targets.push({
      key: `mp:${id}`,
      name: p.name || p.displayName || p.email || 'Guest',
      meetingParticipantId: id,
      avatarUrl: p.avatarUrl ?? null,
    });
  }
  return [...targets, UNASSIGNED_TARGET];
}

/** Whether moving `item` to `target` would change nothing. A null participant
 *  id means either "Unassigned" or an owner the server only knows by name, so
 *  for Unassigned the name decides: a bare-name owner can still be cleared. */
export function isAlreadyAssignedTo(item: MeetingActionItem, target: AssignTarget): boolean {
  if (target.meetingParticipantId !== null) {
    return item.meetingParticipantId === target.meetingParticipantId;
  }
  return item.meetingParticipantId == null && item.name === target.name;
}

const normName = (s: string | undefined): string => (s ?? '').trim().toLowerCase();

/** Whether the signed-in user owns `item` — the same rule the server uses to
 *  put it on their Todo list: their participant row, or, when the item has no
 *  row, a case-insensitive match on their name. */
export function isOwnedBySelf(
  item: MeetingActionItem,
  participants: MeetingParticipantInfo[]
): boolean {
  const self = participants.find((p) => p.self);
  if (!self) return false;
  if (item.meetingParticipantId != null) {
    return item.meetingParticipantId === self.meetingParticipantId;
  }
  const mine = normName(self.name);
  return mine.length > 0 && normName(item.name) === mine;
}

/** Follow-ups keyed by the action-item text they were created from — the only
 *  link the server keeps between the two. On a duplicate the later row wins,
 *  as on the web meeting-notes page. */
export function indexFollowUps(followUps: ActionItemFollowUp[]): Map<string, ActionItemFollowUp> {
  return new Map(followUps.map((f) => [f.description, f]));
}

/** The items the user has not checked off yet. */
export function openActionItems<T extends MeetingActionItem>(
  items: T[],
  followUps: ActionItemFollowUp[]
): T[] {
  const index = indexFollowUps(followUps);
  return items.filter((it) => !index.get(it.item)?.completed);
}
