# Recorder pill attaches to the meetings window while recording

**Date:** 2026-06-11
**Status:** Approved

## Problem

The floating recorder ("waveform" window) opens at a default position and
floats independently. While a recording is running, it should sit attached to
the right edge of the meetings ("library") window, and only float freely when
the meetings window is minimized.

## Behavior

- While a recording is on-going and the library window is open and not
  minimized, the recorder is pinned flush to the library window's right edge,
  vertically centered against its height. Moving or resizing the library
  window drags the recorder along.
- Minimizing the library window releases the recorder: it stays where it is
  and can be dragged anywhere via its drag handle.
- Restoring (unminimizing) the library window re-attaches the recorder,
  snapping it back to the right edge (any dragged position is discarded).
- If the library window is closed (it is destroyed on close) or was never
  open (tray / auto recordings), the recorder floats freely; if the library
  window is (re)opened during an on-going recording, the recorder attaches.

## Design

Tauri emits no minimize event, so attachment runs as a small Rust watcher
task (`recorder_attach.rs`), spawned by `open_waveform_window` when the
waveform window is created:

- Every ~200 ms: exit when the waveform window is gone; idle when the library
  window is absent or minimized; otherwise compute the target outer position
  and call `set_position` only when it differs from the current one.
- Position math lives in a pure `attach_position(lib_pos, lib_size,
  wave_size) -> (x, y)` function with unit tests: right edge flush
  (`lib.x + lib.width`), vertically centered
  (`lib.y + (lib.height - wave.height) / 2`), all in physical pixels.
- No frontend changes. Dragging the recorder while attached is snapped back
  on the next tick by design (minimize first to drag freely).

## Testing

- Rust unit tests for `attach_position` (centering, taller-recorder case).
- Watcher loop is thin glue over window handles; verified via `cargo check`
  and manual/live run.
