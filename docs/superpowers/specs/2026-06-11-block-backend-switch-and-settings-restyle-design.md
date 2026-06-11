# Block backend switching during recording + Settings restyle

**Date:** 2026-06-11
**Branch:** `fix/block-switch-backend`

## Goal

Two changes to the Settings window:

1. The transcription-backend selector must be disabled while a recording is in
   progress, with a visible hint explaining why.
2. The Settings window visuals must match the Meetings (Library) window: warm
   palette, Polymath font, dark accent, soft offset shadows. Layout and window
   configuration (fixed 450×800, native title bar, hide-on-close) are unchanged.

## Part 1 — Block backend switching while recording

### Recording-state signal (Approach A: Rust event)

Recording state already transitions in exactly three places in
`src-tauri/src/commands.rs`; each gains an `app.emit("recording://state", <bool>)`:

- `open_waveform_window` — after `RecordingState.set(source)` → emit `true`.
- `set_tray_recording` — where it calls `RecordingState.clear()` → emit `false`.
- The waveform window's `Destroyed` event handler — after `clear()` → emit
  `false`.

The event is broadcast to all windows, so any view can subscribe. No change to
`recording_state.rs` itself.

### SettingsView changes

`src/views/SettingsView.vue`:

- New `recordingActive` ref.
- Initialization: on mount, check `getAllWebviewWindows()` for a window labeled
  `waveform` (same pattern as `LibraryView.refreshRecordingState`). Refresh the
  same way on window `focus` (covers the persistent hidden window being
  re-shown).
- Live updates: `listen<boolean>('recording://state', ...)` registered with the
  existing listeners in `onMounted`, unlistened in `onUnmounted`.
- UX when `recordingActive`:
  - `.backend-trigger` gets `:disabled="recordingActive"` — grayed out,
    `cursor: not-allowed`, dropdown cannot open.
  - Hint text under the Backend row: "Backend can't be changed while
    recording."
- Guard: `selectBackend()` early-returns when `recordingActive` (defense in
  depth against a stale UI).

## Part 2 — Settings visual restyle to match Meetings

CSS/template-class changes only, inside `SettingsView.vue` scoped styles,
referencing `LibraryView.vue` as the source of truth:

- Background: `#f5f5f7` → `#f7f6f4`.
- Font stack: `-apple-system, system-ui, sans-serif` →
  `Polymath, -apple-system, system-ui, sans-serif`.
- Text: primary `#1d1d1f` → `#1c1c1c`; secondary grays aligned with
  LibraryView's.
- Cards: border-radius 12px, subtle border + soft offset shadow (Meetings
  card/nav-pill treatment) instead of flat blur shadow.
- Accent: indigo `#6366f1`/`#4f46e5` → dark `#1c1c1c` for checked toggles, the
  primary button (solid dark, like the active nav tab), the backend trigger and
  active dropdown option. Success green aligned to `#2e8b4f`.
- Buttons and the backend dropdown menu get the same border + shadow treatment
  as the Meetings nav pill.

Out of scope: window size/resizability, title-bar style, sidebar/section
restructuring, close behavior.

## Error handling

- Window-presence check failures log and leave `recordingActive` unchanged
  (matches LibraryView's behavior).
- The emit calls in Rust are fire-and-forget (`let _ =`), consistent with
  existing emits in `commands.rs`.

## Testing & verification

- Existing test suite stays green.
- Manual verification via the ariso-desktop MCP (`npm run tauri:dev:debug`):
  1. Start a recording; open Settings → backend dropdown disabled with hint.
  2. Stop the recording → dropdown re-enabled, hint gone.
  3. Keep Settings open while a recording starts (event path) → dropdown
     disables live.
  4. Visual check of the restyle against the Meetings window.
