# Library meetings: date grouping, working "Today" tab, start-recording → picker

**Date:** 2026-06-10
**Status:** Approved
**Branch:** `feat/group-meeting-by-date`
**Figma:** Features / node `2827:34482` (meeting list with `UPCOMING` divider)

## Problem

The main meetings window (`LibraryView`) shows meetings in two flat buckets —
`earlier` (most-recent first) and a single `UPCOMING` divider — with no
per-date structure. The bottom nav pill (`Today` / `Meetings` / `Todo`) is
decorative: every tab is hardcoded and none has a click handler. The sidebar
`+` (start-recording) button opens the floating recorder directly, skipping the
meeting picker. The Ariso list window only reaches **+1 day forward**, so the
`UPCOMING` section is almost always empty.

## Goals

1. Group the meetings list under **per-calendar-date headers** (TODAY,
   YESTERDAY, TOMORROW, else `MON, JUN 9`), newest date first, with a single
   `UPCOMING` section for all future meetings — matching the Figma's single
   divider.
2. Make the **Today** nav tab work: filter the list to only today's meetings.
   Keep **Meetings** as the full date-grouped view. **Todo** stays a
   non-functional placeholder.
3. Show real future meetings: extend the Ariso forward window from **+1 day**
   to **+7 days** (keep 7 days back).
4. Make the sidebar **start-recording `+` button** open the **meeting-picker**
   window for backends that use a picker (Ariso); keep opening the recorder
   directly for Local (which has no picker).

Non-goals: redesigning the picker window's visuals, the detail panel, the
recording flow, the Todo tab, or the dark Figma palette (Library stays light).

## Design

### 1. Date grouping (`LibraryView.vue` + a pure helper)

Extract a pure, testable helper (e.g. `src/composables/groupMeetingsByDate.ts`):

```ts
interface MeetingSection { key: string; label: string; items: MeetingListItem[] }
function groupMeetingsByDate(meetings: MeetingListItem[], now: Date): MeetingSection[]
```

Behavior:
- Partition by `new Date(m.timestamp) > now` → **upcoming** vs **history**.
- **History**: bucket by local calendar date (`YYYY-MM-DD`), sections ordered
  newest date first; within a day, newest first. Label via `dateLabel(key, now)`
  → `TODAY` / `YESTERDAY` (relative) else `MON, JUN 9` (uppercased
  weekday+month+day). Future-but-not-today relative label `TOMORROW` only
  applies inside the upcoming bucket, which is collapsed, so it is unused here.
- **Upcoming**: a single section `{ key: 'upcoming', label: 'UPCOMING', items }`
  sorted soonest-first, appended **after** all history sections.
- Invalid/`NaN` timestamps sort to the bottom of history under a stable bucket
  (keep them visible rather than dropping).

`LibraryView` renders `sections` as: for each section, a `.group-label` header
(reusing the existing UPCOMING style) followed by its `.meeting-item` rows. The
existing scroll-fade mask, selection highlight, and `itemSub` formatting are
unchanged.

### 2. Today tab (`LibraryView.vue`)

- Add `const activeView = ref<'today' | 'meetings'>('meetings')`.
- Bind `nav-tab--active` to `activeView` and add `@click` on the Today and
  Meetings tabs to set it. Todo tab unchanged (no handler).
- A `displayedSections` computed:
  - `meetings` → `groupMeetingsByDate(meetings, now)`.
  - `today` → filter `meetings` to today's calendar date, returned as a single
    chronological (soonest-first) section with **no header** (a date header is
    redundant when every row is today). Empty → render the existing
    "No meetings today." hint style.

### 3. Forward window (`useBackend.ts`)

In `arisoMeetingWindow`, change `end.setDate(end.getDate() + 1)` to `+ 7`. This
is the only data-layer change; `listMeetingsInWindow` already sorts and the
grouping handles the wider range.

### 4. Start-recording → picker (`LibraryView.vue` + Rust)

Frontend `startRecording()`:
- Resolve the active backend; if `backend.usesMeetingPicker`, invoke a new
  `open_meeting_picker` command; else keep `invoke('start_recording_window', {})`
  (Local recorder-direct path). Do **not** flip `recording`/hide the panel for
  the picker path — recording hasn't started; the picker drives that next.

Rust (`commands.rs`):
- Extract `open_meeting_picker_window(app: &AppHandle) -> Result<(), String>`
  that shows+focuses an existing `meeting-picker` window or builds it
  (`/#/meeting-picker`, 400×500, non-resizable, centered, skip_taskbar) — the
  same parameters the tray uses today.
- Add `#[tauri::command] open_meeting_picker(app)` delegating to the helper;
  register it in `main.rs`'s `generate_handler!`.
- Refactor `tray.rs` to call `crate::commands::open_meeting_picker_window(&app_main)`
  after its session gate (DRY; behavior unchanged).

## Testing

- Unit-test `groupMeetingsByDate` (Vitest): TODAY/YESTERDAY/relative + absolute
  labels, newest-date-first ordering, upcoming bucket placement and sorting,
  today-filter, empty input, NaN-timestamp handling. Use a fixed `now`.
- Extend/verify `LibraryView.test.ts`: Today vs Meetings toggle changes the
  rendered sections; start-recording button calls `open_meeting_picker` under
  the Ariso backend and `start_recording_window` under Local.

## Risks / notes

- Today's not-yet-started meetings appear under `UPCOMING`, not `TODAY` — this
  is intentional and matches the Figma's single-divider layout.
- `now` is captured once per session (existing pattern); acceptable for labels.
- Local backend gains date headers across its full history "for free."
