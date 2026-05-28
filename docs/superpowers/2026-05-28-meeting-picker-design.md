# Meeting Picker on Start Recording — Design

**Status:** Approved (brainstorming)
**Date:** 2026-05-28
**Owner:** shawn.zhu@ariso.ai

## Summary

Today, clicking **Start Recording** in the system tray opens a small waveform
window and begins recording immediately. The resulting audio is uploaded as a
brand-new desktop meeting.

This change inserts a picker step between the tray click and the recording.
The picker queries the user's scheduled meetings for today and lets the user
either (a) select one — in which case the upload attaches to that meeting — or
(b) skip and record without a meeting, preserving today's behavior.

## Goals

- Let the user attach a recording to a known scheduled meeting at the moment
  recording begins.
- Keep the existing "record without meeting" flow available with one click.
- Avoid blocking recording when no meetings exist or the API is unreachable.

## Non-goals

- Search, filter, multi-day date ranges, or pagination in the picker.
- Calendar integration UI (creating, editing, or syncing meetings).
- Selecting a meeting *after* recording stops.
- Attaching a recording to a past or future meeting (today only).
- Backend changes — the required endpoints already exist:
  - `GET /meetings?start_date=&end_date=`
  - `POST /desktop/meetings/:id/audio/presign`

## User flow

```
Tray "Start Recording"
    │
    ▼
 Session validates
    │
    ▼
 Meeting picker window opens (~400x500)
    │
    ├── User picks a meeting        ─┐
    │                                 │
    ├── User clicks "Record           │
    │   without meeting"              ├──> Waveform window opens,
    │                                 │     recording auto-starts
    └── (List empty/API error) ──────┘
            User clicks "Record
            without meeting"

    Tray now shows recording menu (pause/stop).

 Stop recording
    │
    ▼
 Upload:
    - If meetingId set → POST /desktop/meetings/:id/audio/presign
    - Else            → POST /desktop/meetings/audio/presign (today's behavior)
```

If the user closes the picker window (X / Esc) without choosing or skipping,
the flow is canceled. The tray menu stays in idle state. No recording starts.

## API contracts

### List scheduled meetings (new call)

`GET /meetings?start_date=<ISO>&end_date=<ISO>`

- Date range: local-day boundaries for "today" (00:00:00 → 23:59:59), each
  serialized as a full ISO 8601 string with the user's local UTC offset.
- Auth: existing `Bearer <session>` header is added by `api_request` in
  `src-tauri/src/commands.rs`.

Expected response shape (only the fields we consume are listed; the endpoint
may return more, which we ignore):

```ts
{
  meetings: Array<{
    id: number;
    title: string | null;
    start_time: string;   // ISO 8601
  }>;
}
```

If the response shape diverges from this assumption, the picker shows an
error state and the user can still skip into "record without meeting".

### Upload audio (change)

`useMeetingApi.uploadAudio()` gains an optional `meetingId?: number` in its
options bag:

```ts
uploadAudio(
  audioBlob: Blob,
  options?: {
    title?: string;
    startAt?: string | null;
    endAt?: string;
    meetingId?: number;
  }
): Promise<{ meetingId: number }>;
```

- If `meetingId` is provided → request URL becomes
  `POST /desktop/meetings/${meetingId}/audio/presign`. Body and metadata are
  the same shape as today.
- If not provided → existing `POST /desktop/meetings/audio/presign` flow
  (unchanged).
- The PUT-to-S3 and `/audio/confirm` steps are unchanged.

## UI / components

### `MeetingPickerView.vue` (new)

A standalone view rendered in its own window. States:

| State    | Shown when                              | Body                                                                     |
| -------- | --------------------------------------- | ------------------------------------------------------------------------ |
| Loading  | API call in flight                      | Centered spinner + label                                                 |
| List     | Response has ≥1 meeting                 | Vertical list, each row = `title` + formatted `start_time` (e.g. 9:00 AM). Sorted ascending by `start_time`. |
| Empty    | Response has 0 meetings                 | "No meetings today" message                                              |
| Error    | Request fails or schema doesn't match   | Friendly error message                                                   |

Every state has a footer with one primary button: **Record without meeting**.

In the list state, clicking a row commits the selection. In all other states,
the only action is the skip button.

No search, no filter, no multi-select.

### `WaveformView.vue` (modified)

- On mount, read `meetingId` from the route query (`route.query.meetingId`).
  Parse it to a `number | null`.
- Pass `meetingId` into `meetingApi.uploadAudio()` in `handleStop`.
- All other behavior identical to today.

### Routes (`src/main.ts`)

Add: `{ path: '/meeting-picker', name: 'MeetingPicker', component: MeetingPickerView }`.

### Tray (`src-tauri/src/tray.rs`)

In the `"start_recording"` menu handler, replace the existing branch that
opens the waveform window directly. The new branch:

1. Keeps the same session validation.
2. If valid, opens (or focuses) a `meeting-picker` window:
   - id: `meeting-picker`
   - URL: `/#/meeting-picker`
   - size: ~400x500, centered, normal decorations, resizable: false, not
     always-on-top, not transparent.
3. Does NOT call `set_menu(app, true, ...)`. The tray stays in idle menu state
   until the picker confirms a choice and the waveform window opens.
4. Does NOT emit `tray://start-recording`. Recording auto-starts when the
   waveform view mounts, as today.

### New Tauri command (`src-tauri/src/commands.rs`)

```rust
#[tauri::command]
pub async fn start_recording_window(
    app: tauri::AppHandle,
    meeting_id: Option<i64>,
) -> Result<(), String>;
```

Behavior:
- If a `meeting-picker` window exists, close it.
- If the `waveform` window already exists, no-op (matches today's "don't
  create if already exists" guard).
- Otherwise build the waveform window with URL
  `/#/waveform?meetingId=<id>` when `meeting_id` is provided, else
  `/#/waveform`.
- After creating the window, set the tray menu to the recording state via
  `crate::tray::set_menu(&app, true, false)`.

Register this command in `main.rs`'s `invoke_handler!` block.

### Frontend wiring (`src/composables/useMeetingApi.ts`)

Add:

```ts
interface ScheduledMeeting {
  id: number;
  title: string | null;
  start_time: string;
}

async function listScheduledMeetings(
  startDate: Date,
  endDate: Date
): Promise<ScheduledMeeting[]>;
```

The function builds the query string with `URLSearchParams`, calls
`api.request('GET', ...)`, asserts 200, and returns the `meetings` array
(sorted ascending by `start_time` by the caller — or here; either is fine
since the only caller is the picker).

## Edge cases

- **No meetings today:** picker shows empty state + skip button. Skip leads
  to current behavior (new meeting created).
- **API error / network failure:** picker shows error state + skip button.
  We do not retry automatically.
- **Picker dismissed (X / Esc):** no recording starts; tray menu unchanged.
  This is by design — closing the picker is "cancel".
- **Start Recording clicked while picker already open:** focus the existing
  picker window rather than spawning a second one (mirrors the settings
  window pattern in `tray.rs`).
- **Unauthorized scheduled-meetings call (401):** treated as an error state.
  The session was already validated by `is_session_valid` before opening the
  picker, so this should be rare; we don't try to re-auth from here.
- **Meeting list contains items with `title = null`:** show "Untitled
  meeting" in the row.
- **Tray menu state at upload time:** unchanged — stopping the recording
  flips the tray back to idle exactly as today.

## Testing

- **Smoke (manual):**
  1. With at least one scheduled meeting today: click Start Recording → pick
     a meeting → record briefly → stop → confirm in the API/DB that the audio
     attached to that meeting's id.
  2. With at least one meeting today: click skip → record → stop → confirm a
     new meeting was created (today's behavior).
  3. With no meetings today: confirm empty state, skip path works.
  4. With API offline: confirm error state, skip path works.
  5. Close picker window (X): confirm no recording starts and tray menu
     stays idle.
  6. Click Start Recording twice: confirm the second click focuses the
     existing picker window instead of opening a second one.

- **Type-check:** `npm run tauri:build` (or `vite build` + `cargo check`) must
  pass before merge.

## Risks

- **Schema assumption:** the response shape of `GET /meetings` is best-guess
  here. First implementation pass should log/inspect a real response and
  adjust types if needed.
- **Window-management churn:** introducing a picker window changes the
  ordering of side effects in `tray.rs`. A bug in close/focus ordering could
  leave orphaned windows. Mitigation: factor window creation through a small
  helper and verify the smoke test above.

## Out of scope (explicit)

- Search/filter inside the picker.
- Multi-day or arbitrary date-range queries.
- Editing a scheduled meeting's title from the picker.
- Calendar provider integration UI.
- Backend changes — both endpoints (`GET /meetings`, `POST /desktop/meetings/:id/audio/presign`) are assumed to already exist.
