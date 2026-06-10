# Recorder redesign — vertical pill

**Date:** 2026-06-09
**Status:** Approved (pending spec review)

## Summary

Replace the horizontal recorder bar with a vertical capsule "pill" that shows a
logo, a 5-bar waveform, and a 6-dot drag handle when idle, and expands on hover
to reveal Stop and Pause/Resume controls plus an elapsed-time readout.

The window is pill-sized and resizes dynamically on hover so empty screen area
never blocks clicks to apps underneath.

## Current state (what we're replacing)

- `src/views/WaveformView.vue` — horizontal 320×56 pill: a 32-bar waveform,
  `REC`/`PAUSED` label + MM:SS timer, and inline Pause/Stop buttons. Also hosts
  the uploading / success / fail status block.
- `src-tauri/src/commands.rs` — `open_waveform_window()` builds the `"waveform"`
  window at `inner_size(320.0, 56.0)`, decorations off, always-on-top,
  transparent, no shadow, skip taskbar.
- `src/composables/useRecorder.ts`, `src/composables/useWaveform.ts` — recording
  + audio analysis. **Reused unchanged.**
- `src/assets/icon-r-w.png` — yellow circular ampersand mark used as the logo.

## Visual design

Near-black capsule (`#0d0d0d`), full-radius ends (`border-radius: 24px`),
**48px wide**.

### Collapsed (idle) — 48 × 124 px

Vertical flex column, top → bottom:

1. **Logo** — `icon-r-w.png` (yellow ampersand), ~28px square.
2. **Waveform** — 5 vertical equalizer bars. White (`#ffffff`) while recording,
   dimmed gray (`#4b5563`) while paused. Bars ≈3px wide, 4px gap, height animates
   ~20%–100% of a ~28px band.
3. **Divider** — 1px faint line (`rgba(255,255,255,0.08)`).
4. **Drag handle** — 6 dots in a 2×3 grid, gray (`#6b7280`). Decorative
   affordance; dragging works anywhere on the pill.

No text in the collapsed state.

### Expanded (hover) — 48 × 210 px

On hover the window grows downward and these elements appear between the
waveform and the divider:

1. **Timer** — small monospace MM:SS (`#9ca3af`), shown **only in the expanded
   state**, directly below the waveform.
2. **Stop** button — rounded square (~34px, radius ~8px, fill `#1f1f1f`,
   hover `#2a2a2a`), dark-red square icon (`#f87171`). Calls `handleStop`.
3. **Pause/Resume** button — rounded square, white `‖` pause icon; toggles to a
   filled play/`●` icon when paused. Calls `handlePause` / `handleResume`.

Stop is above Pause (matches the reference image).

## Interaction & window resize

- The **whole pill** is the hover zone. `mouseenter` on the container expands;
  `mouseleave` collapses. Using the whole container (not just the logo/waveform)
  keeps the pill expanded while the cursor travels down to the buttons.
- Resize is driven from the frontend:
  `getCurrentWebviewWindow().setSize(new LogicalSize(48, 210))` on enter,
  `setSize(new LogicalSize(48, 124))` on leave. Tauri keeps the top-left corner
  fixed, so the pill grows/shrinks downward.
- The buttons stop event propagation (`@click.stop.prevent`) and are **not**
  `data-tauri-drag-region`, so clicking them acts instead of dragging.
- Existing tray event listeners (`tray://pause-recording`,
  `tray://resume-recording`, `tray://stop-recording`) and all
  `set_tray_recording` calls are preserved.

## Waveform: 32 bins → 5 bars

`useWaveform` keeps emitting 32 levels. The view buckets them into 5 averaged
bars (6–7 bins each) for display — no change to the composable's public API.

## Status states (minimal)

Replaces the current uploading / success / fail block:

- **Finalizing:** logo stays; the waveform band shows a small spinner.
- **Success:** brief green ✓ over the pill, then the window **auto-closes**.
- **Failure:** red ✗ shown; pill stays open (no Close button) so the user can
  retry/stop or drag it away. The window can still be closed via the tray.

These render within the same 48px-wide pill; the window stays at collapsed
height during status display.

## Files touched

| File | Change |
|------|--------|
| `src/views/WaveformView.vue` | New vertical template, scoped styles, hover→resize logic, 5-bar bucketing, expanded-only timer, minimal status states |
| `src-tauri/src/commands.rs` | `open_waveform_window` initial `inner_size(48.0, 124.0)` |
| `src-tauri/capabilities/default.json` | Add `core:window:allow-set-size` |

`useRecorder.ts` and `useWaveform.ts` are unchanged.

## Out of scope

- `RecorderPanel.vue` (embedded internal panel) — not part of the floating
  recorder redesign; left as-is.
- Logo redesign / new assets — reuse `icon-r-w.png`.

## Risks / notes

- Rapid `setSize` on an always-on-top transparent window can flicker on some
  platforms; a CSS transition on the inner pill height masks the boundary.
- Dropping the always-visible timer is intentional; elapsed time is available on
  hover.
