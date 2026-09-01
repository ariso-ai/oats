import { describe, it, expect } from 'vitest';
import { recentDayKeys, groupActionItemsByDay, type ActionItemEntry } from './actionItemSections';

function meeting(id: string, title: string, timestamp: string) {
  return { id, title, timestamp };
}

function entry(
  id: string,
  title: string,
  timestamp: string,
  items: Array<{ name?: string; item: string }>
): ActionItemEntry {
  return { meeting: meeting(id, title, timestamp), items };
}

describe('recentDayKeys', () => {
  it('returns today first, then the preceding local calendar days', () => {
    expect(recentDayKeys(new Date(2026, 2, 1, 9, 30), 3)).toEqual([
      '2026-03-01',
      '2026-02-28',
      '2026-02-27',
    ]);
  });

  it('returns only today for a single-day window', () => {
    expect(recentDayKeys(new Date(2026, 7, 31, 23, 59), 1)).toEqual(['2026-08-31']);
  });
});

describe('groupActionItemsByDay', () => {
  const now = new Date(2026, 7, 31, 12, 0);

  it('groups rows under dated day headers, newest day first', () => {
    const sections = groupActionItemsByDay(
      [
        entry('7', 'Platform Sync', '2026-08-30T15:00:00', [{ item: 'Draft migration plan' }]),
        entry('9', 'Q3 Pricing Review', '2026-08-31T09:00:00', [{ item: 'Send pricing deck' }]),
      ],
      now
    );

    expect(sections.map((s) => s.key)).toEqual(['2026-08-31', '2026-08-30']);
    expect(sections[0].label).toBe('TODAY · AUG 31');
    expect(sections[1].label).toBe('YESTERDAY · AUG 30');
  });

  it('emits one row per action item, carrying its source meeting', () => {
    const [today] = groupActionItemsByDay(
      [
        entry('9', 'Q3 Pricing Review', '2026-08-31T09:00:00', [
          { name: 'Dana', item: 'Send pricing deck' },
          { item: 'Follow up with finance' },
        ]),
      ],
      now
    );

    expect(today.rows).toHaveLength(2);
    expect(today.rows[0].text).toBe('Send pricing deck');
    expect(today.rows[0].meeting.title).toBe('Q3 Pricing Review');
    expect(today.rows[1].text).toBe('Follow up with finance');
    expect(today.rows[0].key).not.toBe(today.rows[1].key);
  });

  it('orders a day’s meetings earliest first', () => {
    const [today] = groupActionItemsByDay(
      [
        entry('9', 'Afternoon', '2026-08-31T15:00:00', [{ item: 'Later item' }]),
        entry('7', 'Morning', '2026-08-31T09:00:00', [{ item: 'Earlier item' }]),
      ],
      now
    );

    expect(today.rows.map((r) => r.text)).toEqual(['Earlier item', 'Later item']);
  });

  it('keeps a meeting returned by two day requests from listing its items twice', () => {
    const dupe = entry('9', 'Q3 Pricing Review', '2026-08-31T09:00:00', [
      { item: 'Send pricing deck' },
    ]);

    const [today] = groupActionItemsByDay([dupe, { ...dupe }], now);

    expect(today.rows).toHaveLength(1);
  });

  it('drops blank items and meetings left with none', () => {
    const sections = groupActionItemsByDay(
      [
        entry('9', 'Q3 Pricing Review', '2026-08-31T09:00:00', [
          { item: '   ' },
          { item: 'Send pricing deck' },
        ]),
        entry('7', 'Platform Sync', '2026-08-30T15:00:00', [{ item: '' }]),
      ],
      now
    );

    expect(sections).toHaveLength(1);
    expect(sections[0].rows.map((r) => r.text)).toEqual(['Send pricing deck']);
  });

  it('files a meeting with an unusable start time under UNDATED, last', () => {
    const sections = groupActionItemsByDay(
      [
        entry('7', 'Imported', 'not-a-date', [{ item: 'Chase the import' }]),
        entry('9', 'Q3 Pricing Review', '2026-08-31T09:00:00', [{ item: 'Send pricing deck' }]),
      ],
      now
    );

    expect(sections.map((s) => s.label)).toEqual(['TODAY · AUG 31', 'UNDATED']);
  });

  it('returns no sections when nothing came back', () => {
    expect(groupActionItemsByDay([], now)).toEqual([]);
  });
});
