import { describe, it, expect } from 'vitest';
import {
  groupMeetingsByDate,
  groupTodaysMeetings,
  dateLabel,
  upcomingRelLabel,
  isMeetingInProgress,
} from './groupMeetingsByDate';
import type { MeetingListItem } from './useBackend';

// Fixed "now": Wed 2026-06-10 15:00 local.
const NOW = new Date(2026, 5, 10, 15, 0, 0);

function m(id: string, iso: string, end?: string): MeetingListItem {
  return { id, title: `M${id}`, timestamp: iso, ...(end ? { endTimestamp: end } : {}) };
}

describe('dateLabel', () => {
  it('uses relative labels for today/yesterday/tomorrow', () => {
    expect(dateLabel('2026-06-10', NOW)).toBe('TODAY');
    expect(dateLabel('2026-06-09', NOW)).toBe('YESTERDAY');
    expect(dateLabel('2026-06-11', NOW)).toBe('TOMORROW');
  });

  it('uses an uppercased weekday/month/day for other dates', () => {
    // 2026-06-07 is a Sunday.
    expect(dateLabel('2026-06-07', NOW)).toBe('SUN, JUN 7');
  });
});

describe('groupMeetingsByDate', () => {
  it('buckets every meeting by calendar date, newest date first, earliest-first within a date', () => {
    const meetings = [
      m('past1', '2026-06-09T09:00:00'),
      m('today1', '2026-06-10T09:00:00'),
      m('today2', '2026-06-10T11:00:00'),
      m('soon', '2026-06-10T18:00:00'),
      m('future', '2026-06-12T10:00:00'),
    ];
    const sections = groupMeetingsByDate(meetings, NOW);
    // No UPCOMING section: future meetings live under their own date header.
    expect(sections.map((s) => s.label)).toEqual(['FRI, JUN 12', 'TODAY', 'YESTERDAY']);
    // Within a date, earliest-first (ascending).
    expect(sections[1].items.map((x) => x.id)).toEqual(['today1', 'today2', 'soon']);
    expect(sections[0].items.map((x) => x.id)).toEqual(['future']);
  });

  it('returns an empty array for no meetings', () => {
    expect(groupMeetingsByDate([], NOW)).toEqual([]);
  });

  it('keeps NaN-timestamp meetings under an UNDATED bucket at the end', () => {
    const sections = groupMeetingsByDate([m('bad', 'not-a-date'), m('t', '2026-06-10T09:00:00')], NOW);
    const labels = sections.map((s) => s.label);
    expect(labels).toContain('UNDATED');
    expect(labels.indexOf('UNDATED')).toBeGreaterThan(labels.indexOf('TODAY'));
  });

  it('places an in-progress meeting under its date bucket in start order', () => {
    const sections = groupMeetingsByDate(
      [m('live', '2026-06-10T14:30:00', '2026-06-10T15:30:00'), m('past', '2026-06-10T09:00:00')],
      NOW
    );
    expect(sections.map((s) => s.label)).toEqual(['TODAY']);
    expect(sections[0].items.map((x) => x.id)).toEqual(['past', 'live']);
  });
});

describe('groupTodaysMeetings', () => {
  it('splits today into UPCOMING then EARLIER, each earliest-first', () => {
    const meetings = [
      m('y', '2026-06-09T09:00:00'),
      m('up-late', '2026-06-10T18:00:00'),
      m('up-soon', '2026-06-10T16:30:00'),
      m('past-late', '2026-06-10T11:00:00'),
      m('past-early', '2026-06-10T09:00:00'),
      m('bad', 'nope'),
    ];
    const sections = groupTodaysMeetings(meetings, NOW);
    expect(sections.map((s) => s.label)).toEqual(['UPCOMING', 'EARLIER']);
    expect(sections[0].items.map((x) => x.id)).toEqual(['up-soon', 'up-late']);
    expect(sections[1].items.map((x) => x.id)).toEqual(['past-early', 'past-late']);
  });

  it('omits a group when it has no meetings', () => {
    expect(groupTodaysMeetings([m('p', '2026-06-10T09:00:00')], NOW).map((s) => s.label)).toEqual([
      'EARLIER',
    ]);
    expect(groupTodaysMeetings([m('u', '2026-06-10T18:00:00')], NOW).map((s) => s.label)).toEqual([
      'UPCOMING',
    ]);
  });

  it('returns an empty array when nothing is today', () => {
    expect(groupTodaysMeetings([m('y', '2026-06-09T09:00:00'), m('bad', 'nope')], NOW)).toEqual([]);
  });

  it('keeps an in-progress meeting in UPCOMING, ahead of later ones', () => {
    const meetings = [
      m('live', '2026-06-10T14:30:00', '2026-06-10T15:30:00'), // started, not ended → in progress
      m('soon', '2026-06-10T16:00:00'),
      m('done', '2026-06-10T09:00:00', '2026-06-10T10:00:00'), // ended → earlier
    ];
    const sections = groupTodaysMeetings(meetings, NOW);
    expect(sections.map((s) => s.label)).toEqual(['UPCOMING', 'EARLIER']);
    expect(sections[0].items.map((x) => x.id)).toEqual(['live', 'soon']);
    expect(sections[1].items.map((x) => x.id)).toEqual(['done']);
  });
});

describe('isMeetingInProgress', () => {
  it('is true only between start and end', () => {
    expect(isMeetingInProgress(m('a', '2026-06-10T14:30:00', '2026-06-10T15:30:00'), NOW)).toBe(true);
    expect(isMeetingInProgress(m('b', '2026-06-10T15:30:00', '2026-06-10T16:30:00'), NOW)).toBe(false);
    expect(isMeetingInProgress(m('c', '2026-06-10T13:00:00', '2026-06-10T14:00:00'), NOW)).toBe(false);
  });

  it('is false without an end timestamp', () => {
    expect(isMeetingInProgress(m('d', '2026-06-10T14:30:00'), NOW)).toBe(false);
  });
});

describe('upcomingRelLabel', () => {
  it('formats the time until start, scaling minutes → hours → days', () => {
    expect(upcomingRelLabel(m('a', '2026-06-10T15:20:00'), NOW)).toBe('in 20min');
    expect(upcomingRelLabel(m('b', '2026-06-10T17:00:00'), NOW)).toBe('in 2h');
    expect(upcomingRelLabel(m('c', '2026-06-12T15:00:00'), NOW)).toBe('in 2d');
  });

  it('says "Now" for a meeting in progress', () => {
    expect(upcomingRelLabel(m('live', '2026-06-10T14:30:00', '2026-06-10T15:30:00'), NOW)).toBe('Now');
  });

  it('returns an empty string for an invalid start', () => {
    expect(upcomingRelLabel(m('bad', 'nope'), NOW)).toBe('');
  });
});
