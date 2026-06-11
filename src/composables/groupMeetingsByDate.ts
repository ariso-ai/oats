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

/** ms at which a meeting is considered over: its end timestamp when known,
 *  otherwise its start (unknown-duration meetings are "over" once they begin). */
function meetingEndMs(m: MeetingListItem): number {
  if (m.endTimestamp) {
    const end = new Date(m.endTimestamp).getTime();
    if (!Number.isNaN(end)) return end;
  }
  return new Date(m.timestamp).getTime();
}

/** True while a meeting is happening: start <= now < end. Needs a real end. */
export function isMeetingInProgress(m: MeetingListItem, now: Date): boolean {
  if (!m.endTimestamp) return false;
  const start = new Date(m.timestamp).getTime();
  const end = new Date(m.endTimestamp).getTime();
  if (Number.isNaN(start) || Number.isNaN(end)) return false;
  const t = now.getTime();
  return start <= t && t < end;
}

/** Relative label for an upcoming meeting: "Now" while in progress, otherwise
 *  the time until it starts ("in 20min" / "in 2h" / "in 3d"). Empty for an
 *  unparseable start or a meeting that is already over. */
export function upcomingRelLabel(m: MeetingListItem, now: Date): string {
  const start = new Date(m.timestamp).getTime();
  if (Number.isNaN(start)) return '';
  const t = now.getTime();
  if (start <= t) return isMeetingInProgress(m, now) ? 'Now' : '';
  const mins = Math.round((start - t) / 60_000);
  if (mins < 60) return `in ${Math.max(1, mins)}min`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `in ${hours}h`;
  return `in ${Math.round(hours / 24)}d`;
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
    // "Upcoming" = not yet over: future meetings and ones in progress right now.
    if (meetingEndMs(meeting) > nowMs) upcoming.push(meeting);
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
  // Lexical sort == chronological because keys are zero-padded YYYY-MM-DD;
  // .reverse() then puts the newest date first.
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

/** Today's meetings split into UPCOMING (still to come) then EARLIER (already
 *  started), each ordered earliest-first; drops other days and invalid dates. */
export function groupTodaysMeetings(meetings: MeetingListItem[], now: Date): MeetingSection[] {
  const nowMs = now.getTime();
  const today = localDateKey(now);
  const upcoming: MeetingListItem[] = [];
  const earlier: MeetingListItem[] = [];
  for (const meeting of meetings) {
    const d = new Date(meeting.timestamp);
    if (Number.isNaN(d.getTime()) || localDateKey(d) !== today) continue;
    // In-progress meetings (started but not ended) count as upcoming.
    if (meetingEndMs(meeting) > nowMs) upcoming.push(meeting);
    else earlier.push(meeting);
  }

  const byTimeAsc = (a: MeetingListItem, b: MeetingListItem) =>
    new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime();
  upcoming.sort(byTimeAsc);
  earlier.sort(byTimeAsc);

  const sections: MeetingSection[] = [];
  if (upcoming.length) sections.push({ key: 'upcoming', label: 'UPCOMING', items: upcoming });
  if (earlier.length) sections.push({ key: 'earlier', label: 'EARLIER', items: earlier });
  return sections;
}
