import type { MeetingListItem } from './useBackend';

export interface MeetingSection {
  key: string;
  label: string;
  items: MeetingListItem[];
}

function localDateKey(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function dateLabel(key: string, now: Date): string {
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);

  if (key === localDateKey(now)) return 'TODAY';
  if (key === localDateKey(yesterday)) return 'YESTERDAY';
  if (key === localDateKey(tomorrow)) return 'TOMORROW';

  const [y, mo, d] = key.split('-').map(Number);
  return new Date(y, mo - 1, d)
    .toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
    .toUpperCase();
}

/** Bucket meetings under per-calendar-date headers (newest date first) with a
 *  single trailing UPCOMING section for everything starting after `now`. */
export function groupMeetingsByDate(meetings: MeetingListItem[], now: Date): MeetingSection[] {
  const nowMs = now.getTime();
  const history: MeetingListItem[] = [];
  const upcoming: MeetingListItem[] = [];
  for (const meeting of meetings) {
    const ts = new Date(meeting.timestamp).getTime();
    if (!Number.isNaN(ts) && ts > nowMs) upcoming.push(meeting);
    else history.push(meeting);
  }

  const buckets = new Map<string, MeetingListItem[]>();
  for (const meeting of history) {
    const d = new Date(meeting.timestamp);
    const key = Number.isNaN(d.getTime()) ? 'unknown' : localDateKey(d);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(meeting);
    else buckets.set(key, [meeting]);
  }

  const sections: MeetingSection[] = [];
  const datedKeys = [...buckets.keys()].filter((k) => k !== 'unknown').sort().reverse();
  for (const key of datedKeys) {
    const items = [...buckets.get(key)!].sort(
      (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );
    sections.push({ key, label: dateLabel(key, now), items });
  }
  if (buckets.has('unknown')) {
    sections.push({ key: 'unknown', label: 'UNDATED', items: buckets.get('unknown')! });
  }
  if (upcoming.length) {
    upcoming.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
    sections.push({ key: 'upcoming', label: 'UPCOMING', items: upcoming });
  }
  return sections;
}

/** Today's meetings only, soonest-first; drops other days and invalid dates. */
export function todaysMeetings(meetings: MeetingListItem[], now: Date): MeetingListItem[] {
  const today = localDateKey(now);
  return meetings
    .filter((meeting) => {
      const d = new Date(meeting.timestamp);
      return !Number.isNaN(d.getTime()) && localDateKey(d) === today;
    })
    .sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
}
