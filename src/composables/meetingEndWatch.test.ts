import { describe, it, expect } from 'vitest';
import {
  shouldPromptMeetingEnd,
  findMeetingEndAt,
  findNextMeetingStart,
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

describe('shouldPromptMeetingEnd with a next meeting start', () => {
  // Back-to-back: next meeting starts exactly when the current one ends.
  const NEXT = END;

  it('prompts the moment the next meeting starts, without waiting for grace', () => {
    expect(shouldPromptMeetingEnd(END, NEXT, false, 0, null, NEXT)).toBe(true);
  });

  it('prompts at the next start even before the current meeting has ended (overlap)', () => {
    const overlapStart = END - 60_000; // next meeting begins 1 min before current end
    expect(shouldPromptMeetingEnd(END, overlapStart, false, 0, null, overlapStart)).toBe(true);
  });

  it('does not prompt before the next meeting starts (nor before end + grace)', () => {
    expect(shouldPromptMeetingEnd(END, NEXT - 1, false, 0, null, NEXT)).toBe(false);
  });

  it('prompts at the next start when the current meeting has no scheduled end', () => {
    expect(shouldPromptMeetingEnd(null, NEXT, false, 0, null, NEXT)).toBe(true);
  });

  it('never prompts while paused, even when the next meeting has started', () => {
    expect(shouldPromptMeetingEnd(END, NEXT, true, 0, null, NEXT)).toBe(false);
  });

  it('never prompts past the max, even when the next meeting has started', () => {
    expect(shouldPromptMeetingEnd(END, NEXT + MEETING_END_REPROMPT_MS, false, 2, NEXT, NEXT)).toBe(
      false,
    );
  });

  it('still fires the grace-based prompt when the next start is later', () => {
    const lateNext = END + 60 * 60_000;
    expect(
      shouldPromptMeetingEnd(END, END + MEETING_END_GRACE_MS, false, 0, null, lateNext),
    ).toBe(true);
  });
});

describe('findNextMeetingStart', () => {
  const meetings = [
    { id: 1, start_at: '2026-06-28T09:00:00.000Z', end_at: '2026-06-28T10:00:00.000Z', title: 'Standup' },
    { id: 2, start_at: '2026-06-28T09:00:00.000Z', title: 'Parallel' }, // same start as 1
    { id: 3, start_at: '2026-06-28T10:00:00.000Z', title: 'Next' },
    { id: 4, start_at: '2026-06-28T11:00:00.000Z', title: 'Later' },
    { id: 5, start_at: '2026-06-28T08:00:00.000Z', title: 'Earlier' },
    { id: 6, start_at: 'not-a-date', title: 'Bad' },
  ];

  it('returns the earliest meeting starting strictly after the attached one', () => {
    expect(findNextMeetingStart(meetings, 1)).toEqual({
      startAt: Date.parse('2026-06-28T10:00:00.000Z'),
      title: 'Next',
    });
  });

  it('ignores meetings starting at the same time (parallel, not back-to-back)', () => {
    expect(findNextMeetingStart([meetings[0], meetings[1]], 1)).toEqual({
      startAt: null,
      title: null,
    });
  });

  it('ignores earlier meetings and unparseable starts', () => {
    expect(findNextMeetingStart([meetings[0], meetings[4], meetings[5]], 1)).toEqual({
      startAt: null,
      title: null,
    });
  });

  it('returns nulls when the attached meeting is absent from the list', () => {
    expect(findNextMeetingStart(meetings, 99)).toEqual({ startAt: null, title: null });
  });

  it('returns nulls when the attached meeting has no parseable start', () => {
    expect(findNextMeetingStart(meetings, 6)).toEqual({ startAt: null, title: null });
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
