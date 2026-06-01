import type { ScheduledMeeting } from './useMeetingApi';

export type FeaturedKind = 'current' | 'next' | 'none';

export interface DefaultMeeting {
  featured: ScheduledMeeting | null;
  kind: FeaturedKind;
}

const FIVE_MIN_MS = 5 * 60_000;
const SIXTY_MIN_MS = 60 * 60_000;

/**
 * Choose the meeting to feature by default in the picker.
 *
 * A meeting is "current" when  start - 5min <= now <= start + 60min.
 * If several are current (overlap), the one with the latest start_at wins
 * (the meeting you most recently joined). If none is current, the soonest
 * meeting starting after now is "next". Otherwise nothing is featured.
 *
 * `meetings` is assumed sorted ascending by start_at, but this function does
 * not rely on that ordering — it computes the extremes explicitly so it is
 * total and order-independent.
 */
export function pickDefaultMeeting(
  meetings: ScheduledMeeting[],
  now: Date
): DefaultMeeting {
  const nowMs = now.getTime();

  let current: ScheduledMeeting | null = null;
  let currentStart = -Infinity;
  let next: ScheduledMeeting | null = null;
  let nextStart = Infinity;

  for (const m of meetings) {
    const startMs = new Date(m.start_at).getTime();
    if (Number.isNaN(startMs)) continue;

    const isCurrent =
      startMs - FIVE_MIN_MS <= nowMs && nowMs <= startMs + SIXTY_MIN_MS;
    if (isCurrent) {
      if (startMs >= currentStart) {
        current = m;
        currentStart = startMs;
      }
    } else if (startMs > nowMs && startMs < nextStart) {
      next = m;
      nextStart = startMs;
    }
  }

  if (current) return { featured: current, kind: 'current' };
  if (next) return { featured: next, kind: 'next' };
  return { featured: null, kind: 'none' };
}
