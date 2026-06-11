# Embedded recorder strip in the library; pill only when minimized

**Date:** 2026-06-11
**Status:** Approved

## Problem

The recorder pill currently floats next to the meetings window (attached by a
position watcher). Instead, the recording UI should live *inside* the library
window — a horizontal strip at the bottom of the detail panel — and the
floating pill should appear only when the meetings window is minimized (or
closed), where the embedded strip can't be seen.

## Constraint

The library window is destroyed on close, so it cannot host the audio
capture. The waveform window remains the recording host; only its visibility
and the placement of the UI change.

## Behavior

- Starting a recording creates the waveform window hidden when the library
  window is visible (no flash); recording runs in it as before.
- The library's detail panel gains a horizontal recorder strip at its bottom:
  3-bar center-weighted waveform, timer, pause/resume button, stop button,
  plus uploading / success / failure states — the same UX states as the pill.
- The pill window is shown only while the library window is minimized or
  absent; it hides again when the library is visible. It stays freely
  draggable (no attach positioning — that watcher behavior is removed).

## Design

- **Rust** — `recorder_attach.rs` becomes `recorder_pill.rs`: the 200 ms
  watcher now only toggles pill visibility. Pure helper
  `pill_should_show(library_exists, library_minimized) -> bool` (unit
  tested): true when the library is absent or minimized.
  `open_waveform_window` builds the window with `visible(false)` when the
  library exists and is not minimized.
- **State broadcast** — `useRecorder` samples the analyser inside
  `onaudioprocess` (fires ~10/s regardless of window visibility, unlike
  rAF) into a `frameLevels` ref. `WaveformView` broadcasts
  `recorder://state` events: `{ bars, durationSeconds, isPaused, phase }`
  with phase `recording | uploading | success | failed | closed`
  (`closed` emitted on teardown).
- **`RecorderStrip.vue`** (new) — pure mirror rendered inside
  `.detail-wrap`: listens to `recorder://state`, renders the horizontal
  strip, and controls the host via the existing `tray://pause-recording`,
  `tray://resume-recording`, `tray://stop-recording` events. Renders
  nothing until a state event arrives; hides on `closed`.
- **LibraryView** — `.detail-wrap` becomes a column: detail card on top,
  `<RecorderStrip />` at the bottom.
- **Cleanup** — delete the unused legacy `RecorderPanel.vue` + test.

## Testing

- Rust unit tests for `pill_should_show`.
- `RecorderStrip.test.ts`: renders from state events, pause/resume/stop emit
  the tray control events, hides on `closed`, shows upload states.
- `WaveformView.test.ts`: broadcasts `recorder://state` on recorder ticks.
- Existing `LibraryView` tests stay green.
