# Meeting Picker — Current Meeting First — Design

**Status:** Approved (brainstorming) — 2026-05-29
**Branch:** `feat/select-meetings`
**Supersedes the default-view behavior in:** `docs/superpowers/2026-05-28-meeting-picker-design.md`

## Goal

When the meeting picker opens, default to showing the single **current** meeting
instead of a flat list of every meeting today. Offer a **View all** link that
expands to today's full list. If no meeting is current, default to the **nearest
upcoming** meeting. If meetings exist today but none is current or upcoming, show
a short "no meeting happening now" prompt that still exposes View all and the
skip button.

## Background

The picker (`src/views/MeetingPickerView.vue`) currently loads today's scheduled
meetings via `meetingApi.listScheduledMeetings(startDate, endDate)` and renders
them as a flat, chronologically-sorted list. The `ScheduledMeeting` type carries
only `id`, `title`, and `start_at` — there is **no end time or duration** from the
API. Selecting a row (or the skip button) invokes the `start_recording_window`
Tauri command via the existing `choose(id | null)` handler.

This change is **frontend-only**: no API, Tauri command, routing, or
`useMeetingApi` change. It is scoped to `MeetingPickerView.vue` plus one new pure
helper module.

## Decisions

These were settled during brainstorming:

1. **"Current" rule — fixed window.** A meeting is *current* when
   `start_at − 5min ≤ now ≤ start_at + 60min`. Chosen over a "span to next start"
   rule and over adding `end_at` to the backend (no backend work in scope).
2. **View all reveals all of today.** The expander shows the full flat list of
   today's meetings, including ones already past — i.e. the picker's current
   behavior.
3. **All-past case.** Meetings exist today but none is current or upcoming →
   show "No meeting happening now" text alongside the View all link and the
   "Record without meeting" button.
4. **Overlap tiebreak — most recent start.** If more than one meeting matches the
   fixed window simultaneously, the default is the one with the **latest
   `start_at` that is ≤ now** (the meeting you most recently joined). If every
   matching meeting starts in the future-within-5min, the latest start wins.

## Selection logic

A new pure helper isolates the time math from the component so it can be reasoned
about (and later tested) independently.

**Module:** `src/composables/pickDefaultMeeting.ts` — a standalone pure function
exported from its own file (the repo keeps shared logic under `src/composables/`;
there is no `utils/` directory).

```ts
type FeaturedKind = 'current' | 'next' | 'none';

interface DefaultMeeting {
  featured: ScheduledMeeting | null;
  kind: FeaturedKind;
}

function pickDefaultMeeting(
  meetings: ScheduledMeeting[], // assumed sorted ascending by start_at
  now: Date
): DefaultMeeting;
```

Algorithm:

- `nowMs = now.getTime()`.
- For each meeting, `startMs = new Date(m.start_at).getTime()`;
  `isCurrent = startMs - 5*60_000 <= nowMs && nowMs <= startMs + 60*60_000`.
- **Current candidates** = all `isCurrent`. If any, the featured meeting is the
  one with the **maximum `start_at`** among them (most recent start). `kind = 'current'`.
- Else **upcoming** = meetings with `startMs > nowMs`; featured = the one with the
  **minimum `start_at`** (soonest). `kind = 'next'`.
- Else `featured = null`, `kind = 'none'`.

Because the input list is already sorted ascending, "max start among current" is
the last current element and "min future start" is the first element with
`startMs > nowMs`; the implementation may rely on that ordering or compute
explicitly — either is acceptable.

`now` is captured **once** with `new Date()` when the picker mounts and passed in,
so the featured choice is stable for the (short-lived) window. No live ticking.

## View states & layout

`MeetingPickerView.vue` gains a reactive `showAll` ref (default `false`) and a
computed `defaultMeeting = pickDefaultMeeting(meetings, now)`.

Existing states preserved unchanged:

- `loading` — spinner + "Loading meetings…"
- `error` — error icon + "Could not load meetings."
- `empty` — "No meetings today." (reached only when the API returns zero meetings)

New collapsed/default rendering (state === `'list'`, `showAll === false`):

| Condition | Collapsed view |
| --- | --- |
| `kind === 'current'` | One meeting row labeled **"Happening now"** + `View all ▾` + skip |
| `kind === 'next'` | One meeting row labeled **"Up next"** + `View all ▾` + skip |
| `kind === 'none'` | Text **"No meeting happening now"** + `View all ▾` + skip |

Expanded rendering (`showAll === true`): the full flat list of today's meetings
(all, including past), exactly as rendered today, plus a `View less ▴` toggle and
the skip button.

Interactions:

- `View all ▾` / `View less ▴` toggles `showAll`. (Hidden when there are no
  meetings, i.e. the `empty` state.)
- Any meeting row click → existing `choose(m.id)`.
- "Record without meeting" → existing `choose(null)`.
- Existing `isChoosing` guard continues to disable all rows/buttons while a
  selection is in flight.

## Error handling

No new failure modes. The same `try/catch` around `listScheduledMeetings`
governs the `error` state. The helper is pure and total (never throws for any
array + Date input).

## Testing

The repo has **no JS test runner** and this change does not add one (consistent
with the existing meeting-picker plan). Verification:

1. `npm run vite:build` — type-check / build passes.
2. Manual smoke tests, one per row of the layout table:
   - A meeting currently in its `[−5min, +60min]` window → shown as "Happening now".
   - No current meeting, a later one exists → "Up next" shows the soonest.
   - Two overlapping current meetings → the later-starting one is featured.
   - Meetings exist but all are past → "No meeting happening now" + View all + skip.
   - Zero meetings today → "No meetings today." empty state (unchanged).
   - `View all` expands to the full list (incl. past); `View less` collapses.
   - Clicking the featured row, a list row, and skip each start recording correctly.

## Out of scope

- Backend / API changes; adding `end_at` or duration.
- Auto-refresh / live re-evaluation of "current" while the window is open.
- Persisting the `showAll` toggle across opens.
- Any change to `start_recording_window`, routing, or `uploadAudio`.
