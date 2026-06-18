# "Ari will join" Label + Start-Recording Confirmation

**Date:** 2026-06-18
**Branch:** `feat/ari-will-join-confirm` (off `main`)
**Status:** Approved design, pending implementation plan

## Problem

On the **ariso.ai** backend, a scheduled meeting can be flagged
`auto_join_scheduled: true` — meaning Ariso's own notetaker bot ("Ari") will
join the call and take notes server-side. When that's true, a local desktop
recording is redundant. Today the desktop gives the user no signal that Ari is
coming, and lets them start a redundant local recording with no warning.

## Goal

1. **Label:** When a meeting has `auto_join_scheduled` truthy (ariso only),
   show a right-aligned **"Ari will join"** label on the same line as that
   meeting's attendees, in both the meeting detail panel and the home "Up Next"
   card.
2. **Confirm:** When the user starts a recording against such a meeting, show an
   in-app confirmation dialog ("Ari is scheduled to join this meeting and take
   notes. Do you still want to record?") with **[Cancel]** (safe default) and
   **[Record anyway]**. Recording proceeds only on "Record anyway".

## Non-Goals

- No Rust/backend changes — the flag already rides in the `/meetings` payload
  (Rust's `current_meeting_auto_join_from` already reads it).
- No change to the recording pipeline itself — the label is read-only display,
  and the confirm only gates the existing `start_recording_window` invoke.
- No new setting to disable the warning (YAGNI).
- The label is not shown in the meeting picker (only detail + Up Next, per
  design decision); the picker only *uses* the flag to gate its confirm.

## Background (current code)

- The flag is **not plumbed into the frontend at all** today. Only Rust reads
  `auto_join_scheduled` from each item of the `/meetings` list response, with a
  lenient `truthy()` (accepts `bool`, `0/1`, `"true"/"1"`):
  `src-tauri/src/meeting_notifications.rs` (`current_meeting_auto_join_from`,
  `truthy`).
- `useMeetingApi.ts`: `listScheduledMeetings` / `listMeetingsInWindow` /
  `searchMeetings` return the raw `data.meetings` objects cast to
  `ScheduledMeeting[]` / `MeetingSearchResult[]` with **no field reshaping** —
  so a new field on `ScheduledMeeting` flows through automatically.
- `useBackend.ts`: `meetingSummaryToListItem` (line ~183) maps
  `ScheduledMeeting → MeetingListItem`; `ArisoBackend.getMeetingDetail` (line
  ~282) maps the `/meeting-notes/:id` payload `→ MeetingDetail` and receives the
  clicked `MeetingListItem`.
- Attendees rendering:
  - `MeetingDetailView.vue` meta band (lines ~67-88): `duration · attendees ·
    type-chip`; attendees block at lines 72-84 ends with
    `<span class="attendees-label">N Attendees</span>`.
  - `UpNextCard.vue` (lines ~59-92): featured meeting; attendee avatars in the
    head row; "Start Meeting Early" button emits `('start', featured)`.
- Start-recording entry points (all invoke `start_recording_window`):
  - `LibraryView.startRecordingFor(item)` (line ~421) — Up Next "Start Meeting
    Early" (receives the `MeetingListItem`).
  - `LibraryView.startRecording()` (line ~475) — generic record button; resolves
    via `decideRecordingAction` to `record` (with `meetingId`) or `picker`.
  - `MeetingPickerView.choose(meetingId)` (lines ~121 and ~157) — separate
    window; lists `meetings.value: ScheduledMeeting[]`, plus
    `defaultMeeting.featured`.
- Existing confirm-modal pattern to mirror: `SettingsView.vue` lines ~3-15
  (`.download-confirm` overlay + `.download-confirm__card` + secondary/primary
  buttons).

## Architecture

Three small, independently-testable units plus type plumbing. No backend work.

### 1. Flag plumbing (types only)

- `useMeetingApi.ts`: add `auto_join_scheduled?: boolean | number | string` to
  `ScheduledMeeting` (inherited by `MeetingSearchResult`). No method-body
  changes (objects pass through untouched).
- New pure helper **`arisoTruthy(v: unknown): boolean`** mirroring Rust's
  `truthy` (`true` for `true`, non-zero number, `"true"`/`"1"`; else `false`).
  Location: `src/composables/autoJoin.ts` (new, small, with its own test).
- `useBackend.ts`:
  - `MeetingListItem`: add `autoJoinScheduled?: boolean`.
  - `meetingSummaryToListItem`: set
    `item.autoJoinScheduled = arisoTruthy(m.auto_join_scheduled)`.
  - `MeetingDetail`: add `autoJoinScheduled?: boolean`.
  - `ArisoBackend.getMeetingDetail(item)`: set
    `autoJoinScheduled: item.autoJoinScheduled ?? false` (list is the
    authoritative source; the detail endpoint is not relied upon).
  - `LocalBackend.getMeetingDetail`: `autoJoinScheduled: false` (local never
    auto-joins).

### 2. "Ari will join" label — `AriWillJoinTag.vue` (new)

Tiny presentational component: a subtle pill — small bot/sparkle glyph + text
"Ari will join". No props beyond rendering itself; callers gate with `v-if`.

- `MeetingDetailView.vue`: render `<AriWillJoinTag v-if="detail.autoJoinScheduled" class="meta-ari" />`
  as the **last** child of the `.card-meta` band, with `margin-left:auto` so it
  sits on the same line as attendees but pushed to the far right.
- `UpNextCard.vue`: render `v-if="featured.autoJoinScheduled"` right-aligned on
  the featured meeting's attendee row.

Local meetings never carry the flag, so a plain `v-if` suffices (no backend
check needed for display).

### 3. Confirmation — `useAriJoinConfirm()` + `AriJoinConfirmDialog.vue` (new)

- **`AriJoinConfirmDialog.vue`** — presentational modal styled after
  `SettingsView`'s `.download-confirm`. Props: `open: boolean`. Emits:
  `confirm`, `cancel`. Title: "Ari is joining this meeting". Body: "Ari is
  scheduled to join this meeting and take notes. Do you still want to record?".
  Buttons: secondary **Cancel** (emits `cancel`), primary **Record anyway**
  (emits `confirm`).
- **`useAriJoinConfirm()`** composable (`src/composables/useAriJoinConfirm.ts`,
  new) — owns reactive `open` state and a single in-flight promise:
  - `open: Ref<boolean>`
  - `requestConfirm(): Promise<boolean>` — sets `open=true`, returns a promise
    that resolves `true` on `confirm`, `false` on `cancel`.
  - `confirm()` / `cancel()` — resolve the in-flight promise and set
    `open=false` (wired to the dialog's events).
- **`shouldConfirmAriJoin(backendId: BackendId, autoJoinScheduled?: boolean): boolean`**
  — pure decision helper (in `src/composables/autoJoin.ts`):
  `backendId === 'ariso' && autoJoinScheduled === true`.

Each entry point gains the same gate, then renders the dialog from the
composable:

```ts
if (shouldConfirmAriJoin(backend.id, item.autoJoinScheduled) && !(await requestConfirm())) {
  return; // user chose Cancel — do not record
}
await invoke('start_recording_window', { meetingId });
```

- `LibraryView.startRecordingFor(item)`: gate on `item.autoJoinScheduled`
  (backend resolved as it already is).
- `LibraryView.startRecording()`: when `decideRecordingAction` returns
  `kind === 'record'`, look up the source `MeetingListItem` (the selected
  today item or `currentNowItem()`) and gate on its `autoJoinScheduled`. The
  `kind === 'picker'` branch does **not** confirm here — the picker confirms.
- `MeetingPickerView.choose(meetingId)`: look up the chosen meeting in
  `meetings.value` (or `defaultMeeting.featured`) and gate on
  `arisoTruthy(m.auto_join_scheduled)`. (The picker only ever runs for ariso, so
  no backend check needed there; still routes through `shouldConfirmAriJoin`
  with `'ariso'` for consistency.) Both `choose` call sites (lines ~121, ~157)
  are gated.

`LibraryView` and `MeetingPickerView` each render one
`<AriJoinConfirmDialog :open="open" @confirm="confirm" @cancel="cancel" />`
fed by their own `useAriJoinConfirm()` instance.

## Data Flow

`/meetings` JSON (`auto_join_scheduled`) → `ScheduledMeeting` → either
`meetingSummaryToListItem` → `MeetingListItem.autoJoinScheduled` →
`MeetingDetail.autoJoinScheduled` (detail panel label) and Up Next card label;
or, in the picker, read directly off `ScheduledMeeting`. The same
`autoJoinScheduled` value feeds `shouldConfirmAriJoin` at every start-recording
click.

## Error Handling

- Missing/odd `auto_join_scheduled` value → `arisoTruthy` returns `false` →
  no label, no confirm (fail-open to current behavior).
- Confirm dialog `cancel` (or backdrop/Escape) → recording not started; no side
  effects.
- The confirm gate is the only new pre-`invoke` step; the existing
  `try/catch` around each entry point is unchanged.

## Testing

- **Unit (Vitest):**
  - `arisoTruthy`: `true`/`false`/`1`/`0`/`"true"`/`"1"`/`"false"`/`undefined`/
    `null`/arbitrary string — mirrors Rust truthy cases.
  - `shouldConfirmAriJoin`: ariso+true → true; ariso+false/undefined → false;
    local+true → false.
- **Component (Vitest + @vue/test-utils):**
  - `AriWillJoinTag`: trivially renders its text (used via `v-if` upstream).
  - `LibraryView.test.ts`: flagged ariso meeting → clicking start shows the
    dialog and does **not** call `start_recording_window` until "Record anyway";
    unflagged ariso meeting and local meeting → starts immediately (no dialog).
- `npm test` green (minus the unrelated pre-existing failures in
  `UpdateView.test.ts` / `LibraryView.test.ts` — confirm any LibraryView
  failures are the same pre-existing set, not new); `npm run vite:build` clean.

## File Summary

| File | Change |
|---|---|
| `src/composables/useMeetingApi.ts` | add `auto_join_scheduled?` to `ScheduledMeeting` |
| `src/composables/autoJoin.ts` (new) | `arisoTruthy`, `shouldConfirmAriJoin` |
| `src/composables/useBackend.ts` | `autoJoinScheduled?` on `MeetingListItem`/`MeetingDetail`; set in `meetingSummaryToListItem` + both `getMeetingDetail` |
| `src/composables/useAriJoinConfirm.ts` (new) | confirm composable (`open`, `requestConfirm`, `confirm`, `cancel`) |
| `src/views/AriWillJoinTag.vue` (new) | label pill |
| `src/views/AriJoinConfirmDialog.vue` (new) | confirm modal |
| `src/views/MeetingDetailView.vue` | render label in meta band (right-aligned) |
| `src/views/UpNextCard.vue` | render label on attendee row (right-aligned) |
| `src/views/LibraryView.vue` | gate `startRecordingFor` + `startRecording`; host dialog |
| `src/views/MeetingPickerView.vue` | gate both `choose` sites; host dialog |
| tests | `autoJoin.test.ts`, `AriWillJoinTag.test.ts`, additions to `LibraryView.test.ts` |

Convention note: the repo has no `src/components/` directory — every `.vue`
(including shared ones like `ShareMeetingPopover.vue`) lives in `src/views/`, so
the two new components go there too.
