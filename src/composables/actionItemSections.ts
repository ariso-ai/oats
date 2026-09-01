import type { ActionItemEntry, MeetingListItem } from './useBackend';
import { dayLabelWithDate, localDateKey } from './groupMeetingsByDate';

export type { ActionItemEntry };

/** A single Todo row: the item text and the meeting it came from (selecting the
 *  row opens that meeting in the detail pane). The endpoint already scopes items
 *  to the signed-in user, so rows carry no owner. */
export interface ActionItemRow {
  key: string;
  text: string;
  meeting: MeetingListItem;
}

export interface ActionItemSection {
  key: string;
  label: string;
  rows: ActionItemRow[];
}

/** The `YYYY-MM-DD` days the Todo tab asks the API for: today first, then the
 *  preceding local calendar days. One request per key — the endpoint serves a
 *  single day. */
export function recentDayKeys(now: Date, count: number): string[] {
  const keys: string[] = [];
  for (let i = 0; i < count; i++) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    keys.push(localDateKey(d));
  }
  return keys;
}

/** Bucket action items under per-calendar-date headers, newest day first, with
 *  each day's meetings ordered earliest-first. Meetings are deduped by id: the
 *  same meeting can surface in two day requests near a timezone boundary, and
 *  its items must not list twice. Meetings with an unusable start time keep
 *  their items under a trailing UNDATED section rather than vanishing. */
export function groupActionItemsByDay(
  entries: ActionItemEntry[],
  now: Date
): ActionItemSection[] {
  const seen = new Set<string>();
  const buckets = new Map<string, ActionItemEntry[]>();

  for (const entry of entries) {
    if (seen.has(entry.meeting.id)) continue;
    const rows = entry.items.filter((it) => it.item.trim().length > 0);
    if (rows.length === 0) continue;
    seen.add(entry.meeting.id);

    const d = new Date(entry.meeting.timestamp);
    const key = Number.isNaN(d.getTime()) ? 'unknown' : localDateKey(d);
    const bucket = buckets.get(key);
    if (bucket) bucket.push({ meeting: entry.meeting, items: rows });
    else buckets.set(key, [{ meeting: entry.meeting, items: rows }]);
  }

  const toRows = (dayEntries: ActionItemEntry[]): ActionItemRow[] =>
    [...dayEntries]
      .sort(
        (a, b) =>
          new Date(a.meeting.timestamp).getTime() - new Date(b.meeting.timestamp).getTime()
      )
      .flatMap((e) =>
        e.items.map((it, i) => ({
          key: `${e.meeting.id}:${i}`,
          text: it.item.trim(),
          meeting: e.meeting,
        }))
      );

  const sections: ActionItemSection[] = [];
  // Lexical sort == chronological because the keys are zero-padded.
  const dayKeys = [...buckets.keys()].filter((k) => k !== 'unknown').sort().reverse();
  for (const key of dayKeys) {
    sections.push({ key, label: dayLabelWithDate(key, now), rows: toRows(buckets.get(key)!) });
  }
  if (buckets.has('unknown')) {
    sections.push({ key: 'unknown', label: 'UNDATED', rows: toRows(buckets.get('unknown')!) });
  }
  return sections;
}
