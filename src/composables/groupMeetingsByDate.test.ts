import { describe, it, expect } from 'vitest';
import { groupMeetingsByDate, todaysMeetings, dateLabel } from './groupMeetingsByDate';
import type { MeetingListItem } from './useBackend';

// Fixed "now": Wed 2026-06-10 15:00 local.
const NOW = new Date(2026, 5, 10, 15, 0, 0);

function m(id: string, iso: string): MeetingListItem {
  return { id, title: `M${id}`, timestamp: iso };
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
  it('buckets history by calendar date newest-first, with UPCOMING last', () => {
    const meetings = [
      m('past1', '2026-06-09T09:00:00'),
      m('today1', '2026-06-10T09:00:00'),
      m('today2', '2026-06-10T11:00:00'),
      m('soon', '2026-06-10T18:00:00'),
      m('future', '2026-06-12T10:00:00'),
    ];
    const sections = groupMeetingsByDate(meetings, NOW);
    expect(sections.map((s) => s.label)).toEqual(['TODAY', 'YESTERDAY', 'UPCOMING']);
    expect(sections[0].items.map((x) => x.id)).toEqual(['today2', 'today1']);
    expect(sections[2].items.map((x) => x.id)).toEqual(['soon', 'future']);
  });

  it('returns an empty array for no meetings', () => {
    expect(groupMeetingsByDate([], NOW)).toEqual([]);
  });

  it('keeps NaN-timestamp meetings under an UNDATED bucket at the end of history', () => {
    const sections = groupMeetingsByDate([m('bad', 'not-a-date'), m('t', '2026-06-10T09:00:00')], NOW);
    const labels = sections.map((s) => s.label);
    expect(labels).toContain('UNDATED');
    expect(labels.indexOf('UNDATED')).toBeGreaterThan(labels.indexOf('TODAY'));
  });
});

describe('todaysMeetings', () => {
  it('returns only today, soonest-first, dropping other days and NaN', () => {
    const meetings = [
      m('y', '2026-06-09T09:00:00'),
      m('t2', '2026-06-10T18:00:00'),
      m('t1', '2026-06-10T08:00:00'),
      m('bad', 'nope'),
    ];
    expect(todaysMeetings(meetings, NOW).map((x) => x.id)).toEqual(['t1', 't2']);
  });
});
