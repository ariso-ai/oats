import { describe, it, expect } from 'vitest';
import {
  shouldPromptMeetingEnd,
  findMeetingEndAt,
  MEETING_END_GRACE_MS,
  MEETING_END_REPROMPT_MS,
} from './meetingEndWatch';

const END = 1_000_000; // arbitrary epoch-ms "scheduled end"

describe('shouldPromptMeetingEnd', () => {
  it('does not prompt before end + grace', () => {
    expect(shouldPromptMeetingEnd(END, END + MEETING_END_GRACE_MS - 1, false, 0, null)).toBe(false);
  });

  it('prompts at end + grace (first prompt)', () => {
    expect(shouldPromptMeetingEnd(END, END + MEETING_END_GRACE_MS, false, 0, null)).toBe(true);
  });

  it('never prompts while paused', () => {
    expect(shouldPromptMeetingEnd(END, END + MEETING_END_GRACE_MS, true, 0, null)).toBe(false);
  });

  it('never prompts when endAt is null (unattached / no calendar end)', () => {
    expect(shouldPromptMeetingEnd(null, END + MEETING_END_GRACE_MS, false, 0, null)).toBe(false);
  });

  it('does not re-prompt before reprompt interval after the first', () => {
    const firstAt = END + MEETING_END_GRACE_MS;
    expect(
      shouldPromptMeetingEnd(END, firstAt + MEETING_END_REPROMPT_MS - 1, false, 1, firstAt),
    ).toBe(false);
  });

  it('re-prompts once after the reprompt interval', () => {
    const firstAt = END + MEETING_END_GRACE_MS;
    expect(
      shouldPromptMeetingEnd(END, firstAt + MEETING_END_REPROMPT_MS, false, 1, firstAt),
    ).toBe(true);
  });

  it('never prompts past the max (2)', () => {
    const firstAt = END + MEETING_END_GRACE_MS;
    expect(
      shouldPromptMeetingEnd(END, firstAt + 10 * MEETING_END_REPROMPT_MS, false, 2, firstAt),
    ).toBe(false);
  });
});

describe('findMeetingEndAt', () => {
  const meetings = [
    { id: 1, end_at: '2026-06-28T10:00:00.000Z', title: 'Standup' },
    { id: 2, title: 'No end' as string | null }, // no end_at
    { id: 3, end_at: 'not-a-date', title: 'Bad' },
  ];

  it('returns the matched meeting end (epoch ms) and title', () => {
    expect(findMeetingEndAt(meetings, 1)).toEqual({
      endAt: Date.parse('2026-06-28T10:00:00.000Z'),
      title: 'Standup',
    });
  });

  it('returns null endAt when the meeting has no end_at', () => {
    expect(findMeetingEndAt(meetings, 2)).toEqual({ endAt: null, title: 'No end' });
  });

  it('returns null endAt when end_at is unparseable', () => {
    expect(findMeetingEndAt(meetings, 3)).toEqual({ endAt: null, title: 'Bad' });
  });

  it('returns null endAt and null title when the id is absent', () => {
    expect(findMeetingEndAt(meetings, 99)).toEqual({ endAt: null, title: null });
  });
});
