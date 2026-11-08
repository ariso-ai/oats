import type { ActionItemEntry, MeetingActionItem, MeetingListItem } from './useBackend';
import { dayLabelWithDate, localDateKey } from './groupMeetingsByDate';

export type { ActionItemEntry };

/** A single Todo row: the item text and the meeting it came from. Rows carry no
 *  owner: the Ariso backend's endpoint already scopes items to the signed-in
 *  user, and the local backend's vault has no concept of other users' notes to
 *  scope away. */
export interface ActionItemRow {
  /** Stable across list rebuilds and across a move between Open and
   *  Completed: the meeting id plus the item text (with a `#n` suffix for a
   *  repeated text), never a position. */
  key: string;
  text: string;
  meeting: MeetingListItem;
  /** The item itself — its `ref` (how to tick it) and `completion`. */
  item: MeetingActionItem;
}

export interface ActionItemSection {
  key: string;
  label: string;
  rows: ActionItemRow[];
}

/** Every todo as a keyed row, in entry order. Meetings are deduped by id: the
 *  same meeting can surface in two day requests near a timezone boundary, and
 *  its items must not list twice. Blank items are dropped. Keys are assigned
 *  over a meeting's full item list, so a row keeps its key whether it is open
 *  or done. */
export function todoRows(entries: ActionItemEntry[]): ActionItemRow[] {
  const rows: ActionItemRow[] = [];
  const seenMeetings = new Set<string>();
  for (const { meeting, items } of entries) {
    if (seenMeetings.has(meeting.id)) continue;
    seenMeetings.add(meeting.id);
    const seenText = new Map<string, number>();
    for (const item of items) {
      const text = item.item.trim();
      if (!text) continue;
      const n = seenText.get(text) ?? 0;
      seenText.set(text, n + 1);
      const key = n === 0 ? `${meeting.id}:${text}` : `${meeting.id}:${text}#${n}`;
      rows.push({ key, text, meeting, item });
    }
  }
  return rows;
}

/** Bucket rows under per-calendar-date headers of their meeting's start, newest
 *  day first, with each day's meetings ordered earliest-first. Meetings with an
 *  unusable start time keep their rows under a trailing UNDATED section rather
 *  than vanishing. */
export function groupRowsByDay(rows: ActionItemRow[], now: Date): ActionItemSection[] {
  const buckets = new Map<string, ActionItemRow[]>();
  for (const row of rows) {
    const d = new Date(row.meeting.timestamp);
    const key = Number.isNaN(d.getTime()) ? 'unknown' : localDateKey(d);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(row);
    else buckets.set(key, [row]);
  }
  // Stable sort, so a meeting's rows stay together and in item order.
  const byStart = (dayRows: ActionItemRow[]) =>
    [...dayRows].sort(
      (a, b) => new Date(a.meeting.timestamp).getTime() - new Date(b.meeting.timestamp).getTime()
    );

  const sections: ActionItemSection[] = [];
  // Lexical sort == chronological because the keys are zero-padded.
  const dayKeys = [...buckets.keys()].filter((k) => k !== 'unknown').sort().reverse();
  for (const key of dayKeys) {
    sections.push({ key, label: dayLabelWithDate(key, now), rows: byStart(buckets.get(key)!) });
  }
  if (buckets.has('unknown')) {
    sections.push({ key: 'unknown', label: 'UNDATED', rows: buckets.get('unknown')! });
  }
  return sections;
}

/** A completion's `at` as a Date: ISO timestamps as-is, an Obsidian
 *  `YYYY-MM-DD` stamp as that local calendar day. Null when absent/unusable. */
export function parseCompletionDate(at: string | undefined): Date | null {
  if (!at) return null;
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(at);
  const d = day ? new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3])) : new Date(at);
  return Number.isNaN(d.getTime()) ? null : d;
}

export interface TodoSections {
  /** Open items, grouped by source-meeting day. */
  open: ActionItemSection[];
  /** Done items, most recently completed first; undated ones last. */
  completed: ActionItemRow[];
}

/** The Todos view's sections, from `todoRows`. */
export function splitTodoSections(rows: ActionItemRow[], now: Date): TodoSections {
  const time = (r: ActionItemRow) => parseCompletionDate(r.item.completion?.at)?.getTime() ?? 0;
  return {
    open: groupRowsByDay(rows.filter((r) => !r.item.completion), now),
    completed: rows.filter((r) => r.item.completion).sort((a, b) => time(b) - time(a)),
  };
}

export interface TodoMeetingFilter {
  meeting: MeetingListItem;
  openCount: number;
}

export interface TodoMeetingSection {
  key: string;
  label: string;
  meetings: TodoMeetingFilter[];
}

/** The Todos sidebar's filter list: every meeting with open todos, once, with
 *  its open count, under the same day headers (and order) as the Open section. */
export function groupTodoMeetingsByDay(open: ActionItemSection[]): TodoMeetingSection[] {
  return open.map((section) => {
    const byMeeting = new Map<string, TodoMeetingFilter>();
    for (const row of section.rows) {
      const f = byMeeting.get(row.meeting.id);
      if (f) f.openCount++;
      else byMeeting.set(row.meeting.id, { meeting: row.meeting, openCount: 1 });
    }
    return { key: section.key, label: section.label, meetings: [...byMeeting.values()] };
  });
}
