import { describe, it, expect } from 'vitest';
import { ariJoinChip, isCanceledMeetingStatus } from './meetingStatus';

describe('isCanceledMeetingStatus', () => {
  it('matches the backend spelling', () => {
    expect(isCanceledMeetingStatus('cancelled')).toBe(true);
  });

  it('tolerates the American spelling, case, and padding', () => {
    expect(isCanceledMeetingStatus('canceled')).toBe(true);
    expect(isCanceledMeetingStatus('Cancelled')).toBe(true);
    expect(isCanceledMeetingStatus(' CANCELED ')).toBe(true);
  });

  it('leaves every other lifecycle status alone', () => {
    for (const s of ['created', 'joining', 'joined', 'recording', 'paused', 'processing', 'done', 'expired']) {
      expect(isCanceledMeetingStatus(s)).toBe(false);
    }
  });

  it('is false for missing or non-string values', () => {
    expect(isCanceledMeetingStatus(null)).toBe(false);
    expect(isCanceledMeetingStatus(undefined)).toBe(false);
    expect(isCanceledMeetingStatus(1)).toBe(false);
    expect(isCanceledMeetingStatus({})).toBe(false);
  });
});

describe('ariJoinChip', () => {
  const NOW = new Date('2026-06-16T12:00:00Z');
  // A meeting running right now: started at 11:45, ends at 12:30.
  const running = {
    autoJoinScheduled: true,
    startAt: '2026-06-16T11:45:00Z',
    endAt: '2026-06-16T12:30:00Z',
  };

  it('promises the join for a meeting that has not started', () => {
    expect(
      ariJoinChip(
        { autoJoinScheduled: true, startAt: '2026-06-16T13:00:00Z', endAt: '2026-06-16T13:30:00Z' },
        NOW
      )
    ).toBe('will-join');
  });

  it('switches to joined once Ari is in a running meeting', () => {
    expect(ariJoinChip({ ...running, status: 'joined' }, NOW)).toBe('joined');
  });

  it('tolerates case and padding on the joined status', () => {
    expect(ariJoinChip({ ...running, status: ' Joined ' }, NOW)).toBe('joined');
  });

  it('keeps the promise for a running meeting Ari has not joined yet', () => {
    expect(ariJoinChip({ ...running, status: 'joining' }, NOW)).toBe('will-join');
  });

  it('does not claim Ari joined before the meeting starts', () => {
    expect(
      ariJoinChip(
        {
          autoJoinScheduled: true,
          status: 'joined',
          startAt: '2026-06-16T13:00:00Z',
          endAt: '2026-06-16T13:30:00Z',
        },
        NOW
      )
    ).toBe('will-join');
  });

  it('shows no chip once the meeting has ended', () => {
    expect(
      ariJoinChip(
        {
          autoJoinScheduled: true,
          status: 'joined',
          startAt: '2026-06-16T10:00:00Z',
          endAt: '2026-06-16T11:00:00Z',
        },
        NOW
      )
    ).toBeNull();
  });

  it('shows no chip for a meeting the backend marked done', () => {
    expect(ariJoinChip({ ...running, status: 'done' }, NOW)).toBeNull();
  });

  it('shows no chip for a canceled or non-auto-join meeting', () => {
    expect(ariJoinChip({ ...running, status: 'cancelled' }, NOW)).toBeNull();
    expect(ariJoinChip({ ...running, autoJoinScheduled: false }, NOW)).toBeNull();
    expect(ariJoinChip({}, NOW)).toBeNull();
  });

  it('keeps the chip when the end time is missing or unparseable', () => {
    expect(ariJoinChip({ autoJoinScheduled: true, startAt: '2026-06-16T11:45:00Z' }, NOW)).toBe(
      'will-join'
    );
    expect(
      ariJoinChip({ ...running, status: 'joined', endAt: 'not-a-date' }, NOW)
    ).toBe('joined');
  });
});
