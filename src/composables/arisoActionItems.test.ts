import { describe, it, expect } from 'vitest';
import {
  UNASSIGNED_TARGET,
  buildAssignTargets,
  indexFollowUps,
  isAlreadyAssignedTo,
  isOwnedBySelf,
  openActionItems,
} from './arisoActionItems';
import type { MeetingParticipantInfo } from './useBackend';

const me: MeetingParticipantInfo = { name: 'Shawn Zhu', self: true, meetingParticipantId: 1 };
const dana: MeetingParticipantInfo = {
  name: 'Dana',
  meetingParticipantId: 2,
  avatarUrl: 'https://img/dana.png',
};

describe('buildAssignTargets', () => {
  it('offers every attendee with a participant row, then Unassigned', () => {
    const targets = buildAssignTargets([me, dana]);
    expect(targets).toEqual([
      { key: 'mp:1', name: 'Shawn Zhu', meetingParticipantId: 1, avatarUrl: null },
      { key: 'mp:2', name: 'Dana', meetingParticipantId: 2, avatarUrl: 'https://img/dana.png' },
      UNASSIGNED_TARGET,
    ]);
  });

  it('skips attendees an item cannot be written to, and duplicates', () => {
    const targets = buildAssignTargets([
      { name: 'Calendar-only guest', email: 'g@x.com' },
      { name: 'Null row', meetingParticipantId: null },
      dana,
      { ...dana, name: 'Dana again' },
    ]);
    expect(targets.map((t) => t.name)).toEqual(['Dana', 'Unassigned']);
  });

  it('labels an attendee by display name, then email, when they have no name', () => {
    const targets = buildAssignTargets([
      { displayName: 'Speaker label', meetingParticipantId: 3 },
      { email: 'e@x.com', meetingParticipantId: 4 },
    ]);
    expect(targets.map((t) => t.name)).toEqual(['Speaker label', 'e@x.com', 'Unassigned']);
  });
});

describe('isAlreadyAssignedTo', () => {
  const danaTarget = buildAssignTargets([dana])[0];

  it('matches a participant target by participant row id', () => {
    expect(isAlreadyAssignedTo({ item: 'x', name: 'Dana', meetingParticipantId: 2 }, danaTarget)).toBe(true);
    expect(isAlreadyAssignedTo({ item: 'x', name: 'Me', meetingParticipantId: 1 }, danaTarget)).toBe(false);
  });

  it('treats a bare-name owner as movable to Unassigned', () => {
    expect(
      isAlreadyAssignedTo({ item: 'x', name: 'Unassigned', meetingParticipantId: null }, UNASSIGNED_TARGET)
    ).toBe(true);
    expect(
      isAlreadyAssignedTo({ item: 'x', name: 'Bob', meetingParticipantId: null }, UNASSIGNED_TARGET)
    ).toBe(false);
  });
});

describe('isOwnedBySelf', () => {
  it('owns an item assigned to my participant row', () => {
    expect(isOwnedBySelf({ item: 'x', name: 'Whoever', meetingParticipantId: 1 }, [me, dana])).toBe(true);
    expect(isOwnedBySelf({ item: 'x', name: 'Shawn Zhu', meetingParticipantId: 2 }, [me, dana])).toBe(false);
  });

  it('falls back to my name when the item has no participant row, like the server', () => {
    expect(isOwnedBySelf({ item: 'x', name: ' shawn zhu ', meetingParticipantId: null }, [me])).toBe(true);
    expect(isOwnedBySelf({ item: 'x', name: 'Shawn Zhu' }, [me])).toBe(true);
    expect(isOwnedBySelf({ item: 'x', name: 'Dana' }, [me])).toBe(false);
    expect(isOwnedBySelf({ item: 'x' }, [me])).toBe(false);
  });

  it('owns nothing when I am not among the participants', () => {
    expect(isOwnedBySelf({ item: 'x', name: 'Dana', meetingParticipantId: 2 }, [dana])).toBe(false);
  });
});

describe('follow-up matching', () => {
  const followUps = [
    { id: 1, description: 'Send pricing deck', completed: true },
    { id: 2, description: 'Book the venue', completed: false },
  ];

  it('indexes follow-ups by the action-item text they were created from', () => {
    const index = indexFollowUps(followUps);
    expect(index.get('Send pricing deck')?.id).toBe(1);
    expect(index.get('Book the venue')?.completed).toBe(false);
    expect(index.get('Something else')).toBeUndefined();
  });

  it('keeps only the items not yet checked off', () => {
    const open = openActionItems(
      [{ item: 'Send pricing deck' }, { item: 'Book the venue' }, { item: 'Draft the RFC' }],
      followUps
    );
    expect(open.map((i) => i.item)).toEqual(['Book the venue', 'Draft the RFC']);
  });
});
