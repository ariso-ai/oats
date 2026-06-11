# Picker selection shows in the Library detail panel

**Date:** 2026-06-11
**Status:** Approved

## Problem

Choosing a meeting in the meeting-picker window starts the recorder, but the
Library window's detail panel (`.detail-wrap` in `LibraryView.vue`) stays on
whatever was previously selected (or the empty state). The user expects the
picked meeting to appear there once recording starts.

## Design

Single choke point on the Rust side: `open_waveform_window` in
`src-tauri/src/commands.rs` emits an app-wide `recording://started` event with
payload `{ meetingId: number | null }` whenever a new recording window is
actually created (not on the focus-existing early return).

`LibraryView.vue` listens for `recording://started`:

- Calls `setRecording(true)` so the sidebar collapses immediately instead of
  waiting for the next window-focus poll.
- If `meetingId` is non-null, selects the matching sidebar item
  (`MeetingListItem.id === String(meetingId)` — the Ariso backend builds list
  ids from the same scheduled-meeting ids the picker sends). If the item is not
  in the loaded list yet, reload the list once and retry; if still absent, do
  nothing.
- `meetingId: null` ("Record without meeting", tray, local backend, auto
  recordings) leaves the detail panel unchanged apart from the recording-state
  collapse.

## Testing

- `LibraryView.test.ts`: mock `@tauri-apps/api/event`; assert that the event
  selects the matching row, that a null `meetingId` leaves selection alone,
  and that an unknown id triggers one list reload.
- Rust side is a one-line emit; covered by `cargo check` and existing flow.
