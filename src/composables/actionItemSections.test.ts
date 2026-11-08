import { describe, it, expect } from 'vitest';
import {
  groupRowsByDay,
  groupTodoMeetingsByDay,
  splitTodoSections as splitRows,
  todoRows,
  type ActionItemEntry,
} from './actionItemSections';
import type { MeetingActionItem } from './useBackend';

function meeting(id: string, title: string, timestamp: string) {
  return { id, title, timestamp };
}

function entry(
  id: string,
  title: string,
  timestamp: string,
  items: MeetingActionItem[]
): ActionItemEntry {
  return { meeting: meeting(id, title, timestamp), items };
}

const groupActionItemsByDay = (entries: ActionItemEntry[], now: Date) =>
  groupRowsByDay(todoRows(entries), now);
const splitTodoSections = (entries: ActionItemEntry[], now: Date) => splitRows(todoRows(entries), now);

describe('groupRowsByDay over todoRows', () => {
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

describe('splitTodoSections', () => {
  const now = new Date(2026, 7, 31, 12, 0);

  it('puts open items under day sections and done items in Completed', () => {
    const { open, completed } = splitTodoSections(
      [
        entry('9', 'Pricing', '2026-08-31T09:00:00', [
          { item: 'Send deck' },
          { item: 'Book venue', completion: { by: 'user', at: '2026-08-31T10:00:00' } },
        ]),
        entry('7', 'Platform Sync', '2026-08-30T15:00:00', [{ item: 'Draft plan' }]),
      ],
      now
    );

    expect(open.map((s) => s.key)).toEqual(['2026-08-31', '2026-08-30']);
    expect(open[0].rows.map((r) => r.text)).toEqual(['Send deck']);
    expect(completed.map((r) => r.text)).toEqual(['Book venue']);
    // The row keeps its item so the view can read its ref and completion.
    expect(completed[0].item.completion?.by).toBe('user');
  });

  it('orders Completed most recently done first, undated last', () => {
    const { completed } = splitTodoSections(
      [
        entry('9', 'Pricing', '2026-08-31T09:00:00', [
          { item: 'Older', completion: { by: 'user', at: '2026-08-28' } },
          { item: 'Undated', completion: { by: 'user' } },
          { item: 'Newer', completion: { by: 'user', at: '2026-08-30T10:00:00Z' } },
        ]),
      ],
      now
    );

    expect(completed.map((r) => r.text)).toEqual(['Newer', 'Older', 'Undated']);
  });

  it('keys a row by its meeting and text, so the key survives a move between sections', () => {
    const before = splitTodoSections(
      [entry('9', 'Pricing', '2026-08-31T09:00:00', [{ item: 'A' }, { item: 'B' }])],
      now
    );
    const after = splitTodoSections(
      [
        entry('9', 'Pricing', '2026-08-31T09:00:00', [
          { item: 'A', completion: { by: 'user' } },
          { item: 'B' },
        ]),
      ],
      now
    );

    expect(after.completed[0].key).toBe(before.open[0].rows[0].key);
    expect(after.open[0].rows[0].key).toBe(before.open[0].rows[1].key);
  });

  it('keeps duplicate item texts apart', () => {
    const { open } = splitTodoSections(
      [entry('9', 'Pricing', '2026-08-31T09:00:00', [{ item: 'Same' }, { item: 'Same' }])],
      now
    );

    const keys = open[0].rows.map((r) => r.key);
    expect(new Set(keys).size).toBe(2);
  });
});

describe('groupTodoMeetingsByDay', () => {
  const now = new Date(2026, 7, 31, 12, 0);

  it('lists each meeting with open todos once, with its open count, under its day', () => {
    const sections = groupTodoMeetingsByDay(splitTodoSections(
      [
        entry('9', 'Pricing', '2026-08-31T09:00:00', [
          { item: 'A' },
          { item: 'B' },
          { item: 'C', completion: { by: 'user' } },
        ]),
        entry('8', 'Standup', '2026-08-31T08:00:00', [{ item: 'D' }]),
        entry('7', 'Done meeting', '2026-08-30T15:00:00', [{ item: 'E', completion: { by: 'user' } }]),
      ],
      now
    ).open);

    expect(sections).toHaveLength(1);
    expect(sections[0].label).toBe('TODAY · AUG 31');
    expect(sections[0].meetings.map((m) => [m.meeting.title, m.openCount])).toEqual([
      ['Standup', 1],
      ['Pricing', 2],
    ]);
  });
});
