import { describe, it, expect } from 'vitest';
import { parseDefaultMeetingId } from './parseDefaultMeetingId';

describe('parseDefaultMeetingId', () => {
  it('reads a numeric defaultMeetingId from the hash query', () => {
    expect(parseDefaultMeetingId('#/meeting-picker?defaultMeetingId=50')).toBe(50);
    expect(parseDefaultMeetingId('#/meeting-picker?foo=1&defaultMeetingId=7')).toBe(7);
  });
  it('returns null when absent or not a positive integer', () => {
    expect(parseDefaultMeetingId('#/meeting-picker')).toBeNull();
    expect(parseDefaultMeetingId('#/meeting-picker?defaultMeetingId=abc')).toBeNull();
    expect(parseDefaultMeetingId('')).toBeNull();
  });
});
