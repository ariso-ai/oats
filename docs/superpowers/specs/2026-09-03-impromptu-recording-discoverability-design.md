# Impromptu recording discoverability (issue #355)

## Problem

A recording that isn't tied to a calendar event should be exactly as
discoverable and editable as a calendar recording while it's running. Today it
mostly is — but there are three concrete, verifiable gaps, all rooted in the
same cause: **a "homeless" recording (no calendar-matched meeting id yet) has
no stable identity to key the UI off until it finalizes.**

1. **Local (offline) recordings can't be renamed while capturing.** The title
   affordance in `MeetingDetailView.vue` is unconditional
   (`canEditTitle = !!detail.value`), so a user can click to rename a local
   recording's synthetic in-progress row at any time. But
   `rename_local_recording` (`src-tauri/src/commands.rs:2035`) calls
   `storage::read_meta(&dir)`, and that directory's `meta.json` is only ever
   written by `finalize_core`/`finalize_core_with_target`
   (`src-tauri/src/transcribe.rs`) — i.e. after Stop. `local_recording_id_for_start`
   (the command that hands the frontend an id at recording start,
   `src-tauri/src/transcribe.rs:696`) only *computes* what the id will be; it
   creates nothing on disk. So a rename attempted mid-recording fails with
   "recording does not exist," and `MeetingDetailView.commitTitle`'s only
   response is to leave the editor open for a retry that will fail the same
   way until the recording stops. Note-taking during recording is **not**
   affected — `write_recording_note_title`/`write_recording_note` already
   `create_dir_all` on write and read back `""` when the file is missing.

2. **An Ariso (cloud) recording that auto-triggers with no calendar match has
   no identity at all while it runs.** `resolveAuto()` in
   `src/views/WaveformView.vue` only sets `effectiveMeetingId` when
   `resolveAssociation` finds a "current" calendar meeting
   (`src/composables/useAutoTrigger.ts`); otherwise the recording "simply
   proceeds unattached" for its entire session — `effectiveMeetingId` and
   `localRecordingId` both stay `null` in every `recorder://state` heartbeat
   (the broadcast explicitly gates `localRecordingId` on
   `backend.value?.id === 'local'`). Every surface that the issue calls out
   already has working logic *if given an id*, and silently does nothing
   without one:
   - `LibraryView.displayMeetings` only synthesizes a placeholder row when the
     id parses as a local-recording timestamp (`timestampFromLocalRecordingId`)
     — an Ariso session with no id gets no row, no red dot.
   - The titlebar "In recording" indicator renders as an inert
     `<span role="status">` instead of a button specifically because, per its
     own comment, "a recording attached to no meeting has nowhere to re-dock."
   - The floating pill's click (`recording://reveal` → `showRecordingMeeting()`
     in `LibraryView.vue`) is a no-op when `recordingMeetingId` is `null`.
   - `get_active_recording_meeting_id` (used to recover selection when a
     Library window opens mid-recording) returns `None`.
   A manually-started Ariso recording never hits this: the tray/menu-picker
   flow always resolves a real id first, either an existing calendar meeting
   or a freshly created one via `meetingApi.createAudioMeeting()` (the
   "Record a new meeting" path in `MeetingPickerView.vue`, which already
   calls the ad-hoc-meeting endpoint *before* opening the recorder). Only the
   auto-trigger-with-no-match path defers identity to finalize time.

3. **The menu bar shows no meeting identity at all while recording, for any
   recording.** `refresh_tray_title` (`src-tauri/src/tray.rs`) explicitly
   blanks the tray title whenever `RecordingState::is_active()`, and
   `build_recording_menu` offers only Pause/Resume/Stop/Settings/Meetings/Quit
   — no title row, no click-through. `FeaturedMeetingState` (used to render the
   idle "Weekly Sync in 12min" title and the clickable `record_featured` row)
   is only ever populated by the idle `tray_meeting::run_loop`, which stops
   polling once recording starts. So today, "the same usable way as calendar
   recordings" for the menu bar doesn't exist yet for *any* active recording —
   it has to be built, not extended.

## Goal

While any recording is running — local or Ariso, calendar-matched or not —
the user can get back to it from all three surfaces (meetings list, floating
recorder, menu bar), land in its details, rename it if it's untitled, and take
personal notes, exactly as they already can for a calendar-matched recording
that's mid-capture.

Concretely, after this change:
- A brand-new local recording gets a real `meta.json` (status `Recording`) the
  moment it starts, so its rename affordance works immediately, not just after
  Stop.
- An Ariso auto-triggered recording that finds no calendar match gets a real
  ad-hoc meeting id shortly after it starts (once it's known to be a real
  recording, not a mic blip), so it flows through every id-keyed surface that
  already exists for calendar recordings.
- The tray shows which meeting is recording and a click opens it in the
  Meetings window's detail pane, mirroring the idle "click to open" row.

## Non-goals

- No new "impromptu meeting" concept, type, or on-disk/server schema field —
  this reuses the existing local-recording and ad-hoc-meeting mechanisms
  exactly as they already exist for the manual "Record a new meeting" flow.
- No change to which recordings get auto-triggered, matched, or discarded
  (`useAutoTrigger.ts`'s association logic, the 15-minute silence backstop, the
  15-second `MIN_AUTO_DURATION_S` discard) — this only changes *when* a
  session that's already going to be kept gets its id.
- No retroactive fix for recordings already stuck mid-flight when this ships.
- No redesign of the tray's idle countdown UI — the recording-state addition
  is new, parallel structure, not a rework of `build_idle_menu`.
- Windows: `tray.set_title` is already a documented no-op off macOS, so the
  menu-bar title portion of this spec is macOS-only; the recording *menu row*
  (which does render cross-platform) still applies everywhere the tray exists.

## Design

### 1. Local backend — create the recording's identity at start, not at Stop

Add a new command, `local_begin_recording(id: String, created_at: String) ->
Result<(), String>` in `src-tauri/src/commands.rs`, called once from
`WaveformView.vue`'s existing `idResolveToken` watcher
(`src/views/WaveformView.vue:256`) right after `effectiveLocalRecordingId`
resolves — but only for the local backend, and only outside the
`localAppendId` / append-target cases (those ids already have a real
`meta.json` from a prior session; nothing to do).

`local_begin_recording` is idempotent and cheap to make unconditional to call:
- If `recording_dir(&id)` already has a `meta.json` (an append target the
  5-minute-window resolve picked), it's a no-op.
- Otherwise, `create_dir_all` the recording directory and `write_meta` a stub:
  `RecordingMeta { id, title: <timestamp default>, created_at, duration_seconds: 0,
  status: RecordingStatus::Recording, title_is_default: true, .. }`.

`RecordingStatus::Recording` already exists in `src-tauri/src/storage.rs` and
is already read by the frontend's `deriveStage` (`useLocalRecordingProgress.ts:18`,
`s.status === 'recording' || s.status === 'transcribing'` → stage
`'transcribing'`) — it's currently dead code because nothing ever writes it.
Wiring it up here is completing an already-anticipated state, not inventing
one. (See Error handling below for the one behavior change this surfaces:
`deriveStage` needs a small fix so a live `Recording` status doesn't show the
"Generating Transcript" chip.)

Once the stub exists, `rename_local_recording` and the existing rename UX in
`MeetingDetailView.vue` need no changes — `read_meta`/`write_meta` just work.
`local_finalize_recording`'s "new recording" path (not appending) already
resolves the same id via `sanitize_iso_to_id(created_at)` /
`resolve_local_recording_id`, so it overwrites/extends this exact directory —
no self-append risk, since the stub's `status: Recording` correctly fails
`most_recent_appendable`'s `status == Done` check, same as `Transcribing` or
`Failed` today.

`list_local_recordings` has no status filter, so the recording appears in the
real list immediately — `LibraryView.displayMeetings`'s synthetic-row branch
already skips synthesizing when the real list already has the id
(`!base.some((m) => m.id === id)`), so there's no duplicate row; this is a
strict improvement over the current synthetic placeholder (real `hasNote`,
`hasTranscript: false` from disk instead of a hardcoded placeholder).

### 2. Ariso backend — give an unmatched auto-trigger recording a real id

In `resolveAuto()` (`src/views/WaveformView.vue:494`), when
`resolveAssociation` returns `{ kind: 'confirm' }` for the Ariso backend (no
calendar match), call `meetingApi.createAudioMeeting()` — the same endpoint
`MeetingPickerView.vue`'s "Record a new meeting" already uses — and set
`effectiveMeetingId.value` to the returned id. From that point on this session
is indistinguishable, to every downstream surface, from a manually-started
ad-hoc recording: `displayMeetings`'s `pinRecordedMeeting`, the titlebar
button, `recording://reveal` → `showRecordingMeeting()`, and
`get_active_recording_meeting_id` all already work correctly once
`effectiveMeetingId`/`RecordingState.meeting_id` is non-null — see §3 for
propagating the id to the native side.

**Timing:** don't create the meeting the instant `resolveAuto()` runs (which
is before we know whether this is a real meeting or a two-second mic blip).
Defer the `createAudioMeeting()` call until the recording survives past
`MIN_AUTO_DURATION_S` (15s, the existing discard threshold at
`src/views/WaveformView.vue:188`) using the same duration timer already
running. This sidesteps a hard problem for free: there is currently no
"delete meeting" endpoint to clean up an eagerly-created ad-hoc meeting for a
recording that gets discarded, so avoid ever creating one for a session that
might not survive. A manually-triggered recording (never auto, never
duration-gated) already has its id resolved before capture starts via the
picker, so this timing change only affects the auto+unmatched case.

Local's `resolveAuto()` branch is untouched — `resolveAssociation` already
always returns `{ kind: 'confirm' }` for local, which is what §1 now handles
correctly on its own path.

### 3. Menu bar — show and route to the active recording

Extend `RecordingState` (`src-tauri/src/recording_state.rs`) with a
`recording_title: Mutex<Option<String>>` alongside the existing `meeting_id`,
and a setter the frontend can call once it knows what's recording:

```rust
pub fn set_recording_meeting(&self, meeting_id: Option<i64>, title: Option<String>);
```

Call it from a new/extended Tauri command (extending `set_tray_recording` with
optional `meetingId`/`title` args, since it's already the one place recording
state reaches native code) from `WaveformView.vue`'s existing
`watch(effectiveMeetingId, ...)` handler (`src/views/WaveformView.vue:248`),
whenever a real id becomes known — the calendar-matched case (already fires
today), §2's newly-created ad-hoc case, and the "Record a new meeting" /
picker-selected case (covers what manual recordings do today, for symmetry).
Local recordings pass `title` (from `effectiveLocalRecordingId`'s resolved
default) with no `meeting_id`, since local has no server-side meeting to
route a click to a *number* for — routing instead goes through the same
`recording://reveal` broadcast used everywhere else.

`build_recording_menu` (`src-tauri/src/tray.rs:342`) gains a clickable title
row above Pause/Stop, built from `RecordingState`'s new fields, mirroring
`build_idle_menu`'s `record_featured` row: enabled title item + a disabled
gray sub-row (elapsed time or "Recording" for local, since local has no
scheduled time range to show). Menu id `"show_recording"`; its handler in
`create_tray`'s `on_menu_event` (`src-tauri/src/tray.rs:186`) does exactly what
the pill's `showMeetings()` already does — `create_library_window` then emit
`"recording://reveal"` — so it reuses the same reveal path the floating pill
and the meetings-list badge already use, rather than inventing a second route
to the same place.

`refresh_tray_title` keeps blanking the *title-bar text* while recording
(unchanged — that space is reserved for the idle countdown convention), but
the *menu* now always carries the recording identity, giving parity with the
idle state's "meeting name is one click away" affordance without overloading
the title bar with two different meanings.

Untitled recordings (local or the new Ariso ad-hoc case, before the user
renames) show "Untitled meeting" here, matching
`tray_meeting::truncate_title`'s existing fallback and `ArisoBackend.listMeetings`'s
`title || 'Untitled meeting'`.

## Cloud vs offline

Both modes are covered, by different halves of this design:
- **Offline (local):** §1 only. No network calls; the fix is purely about
  writing the on-disk identity earlier.
- **Cloud (Ariso):** §2 only, and only for the auto-trigger/no-match path —
  every other Ariso recording start (calendar-matched, manual picker,
  "Record a new meeting") already resolves an id before or at capture start.
- **§3 (tray)** applies to both, using whichever half already resolved an id/title.

## Error handling

- **`local_begin_recording` fails** (disk full, permissions, vault
  unreachable): swallow and log in the frontend, exactly like the existing
  fallback in the `idResolveToken` watcher's `catch` — recording proceeds
  without a rename affordance until Stop succeeds normally (today's behavior),
  rather than blocking or aborting the recording over a cosmetic feature.
- **`createAudioMeeting()` fails** at the 15s mark (offline blip, expired
  session, server error): log and leave the session unattached for its
  remaining duration — identical to today's total lack of an id, not a
  regression. It does not retry mid-session; the next natural checkpoint is
  finalize, which already creates a meeting server-side if none is attached.
- **`deriveStage` regression risk:** once `RecordingStatus::Recording` is
  actually produced (§1), `deriveStage` (`useLocalRecordingProgress.ts:18`)
  currently folds it into the `'transcribing'` stage, which would incorrectly
  show a "Generating Transcript" spinner chip on a recording that hasn't even
  stopped yet. Fix `deriveStage` to map `status === 'recording'` to `'idle'`
  (no chip — `RecorderStrip`/the pill already communicate "recording"), and
  keep `'transcribing'` mapping only actual transcription.
- **Race: rename lands while `local_finalize_recording` is mid-write.** Both
  already go through `write_atomic`; a rename that lands between finalize's
  own reads/writes either sees the pre- or post-finalize title, never a torn
  file. No new handling needed beyond what atomic writes already provide.
- **Tray menu click while the recording ends in the same instant:** if Stop
  races the click, `"recording://reveal"` still resolves to whatever
  `LibraryView`'s post-stop reload leaves selected (falls back to the first
  meeting, per the existing `watch(recordingMeetingId, ...)` handler) rather
  than erroring — same as clicking the pill or list row today at that boundary.

## Testing

- **Rust unit (`src-tauri/src/commands.rs`, `src-tauri/src/storage.rs`):**
  `local_begin_recording` — creates a fresh stub when absent; no-op (title/
  status preserved) when the id already has a `meta.json` (append-target
  case); rejects a traversal id like the existing `recording_dir` tests.
  `rename_local_recording` on a stub created this way succeeds (regression
  test for the bug this fixes).
- **Rust unit (`src-tauri/src/recording_state.rs`):** `set_recording_meeting`/
  clear round-trip, mirroring the existing `meeting_id_round_trips_and_clears`
  test.
- **Rust unit (`src-tauri/src/tray.rs`):** `build_recording_menu` includes the
  title row with the right label when a recording title is set, and omits it
  (or shows "Untitled meeting") when not — parallel to the existing
  `build_idle_menu`/`truncate_title` tests.
- **Vitest (`WaveformView.test.ts`):** local backend calls
  `local_begin_recording` (or the chosen command name) once `effectiveLocalRecordingId`
  resolves, and skips it for an append target; Ariso auto+no-match calls
  `createAudioMeeting` only after crossing `MIN_AUTO_DURATION_S`, not before;
  a matched or manually-started Ariso recording never calls it.
- **Vitest (`useLocalRecordingProgress.test.ts`):** `deriveStage('recording')`
  → `'idle'`, not `'transcribing'`.
- **Vitest (`LibraryView.test.ts`):** an Ariso session whose
  `recorder://state` heartbeat carries a freshly-resolved `meetingId` (no
  prior calendar id) gets pinned, docks the recorder strip, and the titlebar
  badge renders as a clickable button — reusing the existing
  local-recording-id tests as the template, since the code path is now shared.
- **Manual verification (oats-desktop MCP, both backends):** start a local
  recording, immediately rename it from the detail pane before Stop (currently
  fails; should succeed) and add a personal note while it's still capturing;
  simulate/force an unmatched auto-trigger on Ariso (or temporarily stub
  `resolveAssociation`) and confirm the meetings list, titlebar badge, floating
  pill click, and tray menu row all resolve to the same meeting within ~15s of
  start; confirm the tray row's click opens the Meetings window on that
  meeting's detail pane, and that stopping the recording removes the tray row
  and (for local) that `deriveStage` never showed a spurious transcript chip.

## Open questions

1. Should the tray's recording-title row truncate the same as the idle
   countdown (`truncate_title`'s 10-character cutoff), or can it afford to be
   longer now that it isn't paired with a countdown string in the same line?
2. `set_tray_recording`'s signature grows two more optional args in this
   design — acceptable, or would a dedicated `set_recording_meeting` command
   be preferred to keep `set_tray_recording` focused on pause/resume state?
3. For the local stub's default title before finalize (or a user rename):
   reuse `timestampTitle(createdAt)` (matches what finalize already defaults
   to today) — confirm that's still the desired placeholder wording rather
   than a plain "Untitled meeting" to match the Ariso ad-hoc case introduced
   here.
