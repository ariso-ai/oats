import { describe, it, expect } from 'vitest';
import { resolveAssociation } from './useAutoTrigger';
import type { ScheduledMeeting } from './useMeetingApi';

const now = new Date('2026-06-10T10:00:00Z');

function meeting(id: number, startAt: string): ScheduledMeeting {
  return { id, title: `M${id}`, start_at: startAt };
}

describe('resolveAssociation', () => {
  it('local backend always falls back to confirm', () => {
    const current = [meeting(1, '2026-06-10T09:58:00Z')];
    expect(resolveAssociation('local', current, now)).toEqual({ kind: 'confirm' });
  });

  it('ariso with a current meeting matches it', () => {
    const meetings = [meeting(7, '2026-06-10T09:58:00Z')]; // within -5/+60min
    expect(resolveAssociation('ariso', meetings, now)).toEqual({
      kind: 'matched',
      meetingId: 7,
    });
  });

  it('ariso with only a future meeting falls back to confirm', () => {
    const meetings = [meeting(8, '2026-06-10T12:00:00Z')];
    expect(resolveAssociation('ariso', meetings, now)).toEqual({ kind: 'confirm' });
  });

  it('ariso with no meetings falls back to confirm', () => {
    expect(resolveAssociation('ariso', [], now)).toEqual({ kind: 'confirm' });
  });
});
