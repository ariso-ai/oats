# Meetings Main Window — Split Layout + Embedded Recorder

**Date:** 2026-06-09
**Status:** Approved (design)

## Summary

Evolve the existing `library` window into a two-pane main window for meetings.
The left pane shows a backend-driven meeting list; the right pane is an (empty for
now) detail area with a **Record** button in its top-right corner. Clicking Record
starts capturing immediately and docks an embedded recorder at the bottom of the
right pane. The recorder is hidden whenever recording is not active.

This is the first increment of a larger consolidation of the app's transient
tray-spawned windows into a single main window. The right-pane detail, row
selection, and retirement of the floating recorder / meeting-picker / tray
"Start Recording" flow are intentionally deferred.

## Context

The app is a macOS menubar/tray app with **no persistent main window**. It spawns
transient `WebviewWindow`s on demand (`src-tauri/src/commands.rs`):

- `library` → `LibraryView.vue` (route `/library`): today a single-column
  "Meetings" list of **local recordings** via `list_local_recordings`.
- `waveform` → `WaveformView.vue` (route `/waveform`): the recorder — a floating,
  always-on-top, transparent 320×56 window that manages tray state
  (`set_tray_recording`), handles the upload/transcribe UI, listens to
  `tray://*` events, and **closes itself** on stop.
- plus `settings`, `meeting-picker`, `oauth`, `update`, `bootstrap`.

Backends are abstracted behind the `Backend` interface (`src/composables/useBackend.ts`):
`ArisoBackend` (server, needs auth) and `LocalBackend` (on-device). The active
backend is resolved via `getActiveBackend()` from the `settings.json` store.

## Decisions

1. **Evolve `LibraryView.vue` in place** (keep route `/library`, the
   `create_library_window` command, and the window label) rather than adding a
   separate `main` window. Avoids a duplicate meetings window.
2. **Left list is backend-driven**:
   - Ariso → `GET /meetings?start_date=…&end_date=…` covering **now − 7 days …
     now + 24 h**.
   - Local → `list_local_recordings`.
3. **Record button starts recording immediately**: clicking the top-right button
   reveals the embedded recorder at the bottom of the right pane *and* begins
   capture. Stopping hides the recorder again. ("When not recording, do not show
   the recorder.")
4. **Build a separate `RecorderPanel.vue`** for the embedded case rather than
   refactoring the floating `WaveformView.vue`, so the working tray recorder is
   left untouched.

## Design

### 1. Layout (`LibraryView.vue`)

`LibraryView` becomes a horizontal flex split filling the window:

- **Left panel** — fixed width (~300px), the existing scrollable meeting list with
  current row styling reused. `min-height: 0` / `overflow-y: auto` preserved.
- **Right panel** — flex-grow. Contains:
  - a header bar with a circular **Record** button pinned top-right;
  - an empty body (placeholder for future meeting detail);
  - the embedded `RecorderPanel` docked at the bottom, mounted only while recording.
- `create_library_window` default inner size bumped 460×560 → **~900×600** so the
  split has room. Window stays `resizable(true)`, `skip_taskbar(true)`, title
  "Meetings".

### 2. Backend-driven list (`useBackend.ts`)

Add to the `Backend` interface:

```ts
interface MeetingListItem {
  id: string;            // string for uniform :key; local ids are strings, ariso numeric ids stringified
  title: string;
  subtitle: string;      // formatted date/time
  status?: RecordingSummary['status'];  // present for local; omitted for ariso for now
}

interface Backend {
  // …existing…
  listMeetings(): Promise<MeetingListItem[]>;
}
```

- `LocalBackend.listMeetings()` → `local.listRecordings()`, map each
  `RecordingSummary` to `{ id, title, subtitle: formatDate(createdAt) +
  duration, status }`.
- `ArisoBackend.listMeetings()` → `GET /meetings?start_date=<YYYY-MM-DD>&
  end_date=<YYYY-MM-DD>` with `start_date = today − 7d`, `end_date = today + 1d`;
  map `{ meetings: [{ id, title, start_at }] }` to
  `{ id: String(id), title: title || 'Untitled meeting', subtitle: formatTime(start_at) }`.
- `LibraryView` calls `getActiveBackend().listMeetings()` on mount instead of
  `local.listRecordings()` directly. Loading / empty / error states preserved.

**Open contract detail (confirm during build):** existing working code
(`useMeetingApi.listScheduledMeetings`) calls `/meetings?startDate=<ISO>&
endDate=<ISO>` (camelCase, ISO datetime). This spec follows the literal request
form `start_date`/`end_date` (snake_case, date-only). If the backend rejects the
snake_case/date-only form, fall back to the proven camelCase ISO variant with the
same (now−7d … now+24h) window — a one-line change.

### 3. Embedded recorder (`RecorderPanel.vue`)

New component reusing the real recording logic, **without** the floating-window /
tray lifecycle:

- Uses `useRecorder`, `useWaveform`, and `getActiveBackend()`.
- On mount: resolve active backend, `recorder.startRecording(mode)` (mode derived
  from recording settings as in `WaveformView`), start the waveform analyser, and
  show REC state + pause/stop controls. Reuse `WaveformView`'s bar/timer markup.
- **No** `set_tray_recording`, **no** `tray://*` listeners, **no** window close.
- On stop: `recorder.stopRecording()` → `backend.finalizeRecording(blob, meta)`
  with `meetingId` undefined (a fresh meeting/recording). Show a brief
  uploading/transcribing → success/failed state inline, then emit a `done` event.
- `LibraryView` owns visibility: `recording` ref toggled true by the Record
  button, false on the panel's `done` event; on `done` it refreshes the left list.
- Recording-source guard (both sources disabled / settings unreadable) mirrors
  `WaveformView.startRecording`: abort and collapse the panel instead of recording
  silence.

The floating `WaveformView` + tray "Start Recording" flow is unchanged and still
available.

### 4. Files touched

- `src/views/LibraryView.vue` — split layout, backend-driven list, record toggle.
- `src/composables/useBackend.ts` — `MeetingListItem`, interface method, two impls.
- `src/views/RecorderPanel.vue` — **new** embedded recorder.
- `src-tauri/src/commands.rs` — `create_library_window` size bump only.

Unchanged: `/library` route, window label, `WaveformView.vue`, tray flow,
meeting-picker.

### 5. Testing (vitest)

- Unit-test `LocalBackend.listMeetings` and `ArisoBackend.listMeetings`
  normalizers: shape mapping, and the Ariso date window (`start_date` = −7d,
  `end_date` = +1d) and param form.
- `LibraryView` test: recorder panel hidden on load, shown after the Record button
  is clicked, hidden again after the panel emits `done`, and the list refreshes.
- Existing `LibraryView.test.ts` updated for the backend-driven data source.

## Out of scope (deferred)

- Right-pane meeting detail and row selection.
- Retiring the floating recorder, meeting-picker, or tray "Start Recording" flow.
- Editing/attaching the embedded recording to a pre-existing meeting (always
  creates a new one for now).
