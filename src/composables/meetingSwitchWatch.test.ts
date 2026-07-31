import { describe, it, expect } from 'vitest';
import {
  findNextMeeting,
  shouldPromptMeetingSwitch,
  MEETING_SWITCH_PROMPT_TIMEOUT_MS,
} from './meetingSwitchWatch';

const T0 = Date.parse('2026-07-21T10:00:00Z');
const meeting = (id: number, startOffsetMin: number | null, title: string | null = null) => ({
  id,
  start_at: startOffsetMin === null ? null : new Date(T0 + startOffsetMin * 60_000).toISOString(),
  title,
});

describe('shouldPromptMeetingSwitch', () => {
  it('fires the moment the next meeting starts, not before', () => {
    expect(shouldPromptMeetingSwitch(T0, T0 - 1, false, false)).toBe(false);
    expect(shouldPromptMeetingSwitch(T0, T0, false, false)).toBe(true);
    expect(shouldPromptMeetingSwitch(T0, T0 + 60_000, false, false)).toBe(true);
  });

  it('is frozen while paused', () => {
    expect(shouldPromptMeetingSwitch(T0, T0 + 1, true, false)).toBe(false);
  });

  it('never re-prompts for an already-offered candidate', () => {
    expect(shouldPromptMeetingSwitch(T0, T0 + 1, false, true)).toBe(false);
  });
});

describe('findNextMeeting', () => {
  it('picks the earliest meeting after the attached start, carrying id and title', () => {
    const meetings = [meeting(1, 0, 'Current'), meeting(2, 60, 'Later'), meeting(3, 30, 'Next')];
    expect(findNextMeeting(meetings, 1)).toEqual({ id: 3, startAt: T0 + 30 * 60_000, title: 'Next' });
  });

  it('normalizes a missing candidate title to null', () => {
    const meetings = [meeting(1, 0), meeting(2, 30)];
    expect(findNextMeeting(meetings, 1)).toEqual({ id: 2, startAt: T0 + 30 * 60_000, title: null });
  });

  it('skips parallel meetings sharing the attached start', () => {
    const meetings = [meeting(1, 0), meeting(2, 0, 'Parallel')];
    expect(findNextMeeting(meetings, 1)).toBeNull();
  });

  it('returns null when the attached meeting is missing or has no parseable start', () => {
    expect(findNextMeeting([meeting(2, 30)], 1)).toBeNull();
    expect(findNextMeeting([{ id: 1, start_at: 'nope', title: null }, meeting(2, 30)], 1)).toBeNull();
    expect(findNextMeeting([meeting(1, null), meeting(2, 30)], 1)).toBeNull();
  });

  it('ignores candidates with missing or unparseable starts', () => {
    const meetings = [meeting(1, 0), meeting(2, null), { id: 3, start_at: 'nope', title: null }];
    expect(findNextMeeting(meetings, 1)).toBeNull();
  });
});

describe('MEETING_SWITCH_PROMPT_TIMEOUT_MS', () => {
  it('matches the Rust MEETING_SWITCH_PROMPT_SECONDS cosmetic countdown', () => {
    expect(MEETING_SWITCH_PROMPT_TIMEOUT_MS).toBe(30_000);
  });
});
