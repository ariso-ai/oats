import { describe, it, expect } from 'vitest';
import { isCanceledMeetingStatus } from './meetingStatus';

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
