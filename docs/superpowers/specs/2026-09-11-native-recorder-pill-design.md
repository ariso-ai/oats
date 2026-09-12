# Native recorder pill

**Date:** 2026-09-11
**Status:** Approved
**Issue:** #390

## Summary

Replace the webview-rendered floating recorder pill with a natively drawn one:
AppKit + SwiftUI on macOS, Win32 + Direct2D on Windows. The small pill is hard
to hit with the cursor today — a click on the unfocused webview is often eaten
by focus, and only a tiny dot grid drags. A native panel takes the first click,
never steals focus, and drags from anywhere.

Ships as two stacked PRs:

1. **macOS (this PR)** — the shared Rust bridge, the headless-recorder change to
   the `waveform` webview, and the Swift pill. Windows keeps the Vue pill.
2. **Windows** — the Win32/Direct2D pill on top of the same bridge; deletes the
   Vue pill.

## What stays the same

The `waveform` webview keeps owning the recording session: capture, the
`recorder://state` broadcast, finalize/upload, retry/resume, the silence and
meeting-end prompts, and the `recorder://yield` handshake. Only its *painting*
moves to native code. Visibility rules are unchanged (see
`recorder_pill::should_show_now`): the pill shows while the Meetings window is
closed or minimized, docks to the primary screen's right edge on reveal, and
lives exactly as long as the `waveform` window. The visual design matches
`2026-06-09-recorder-vertical-pill-design.md` as it is implemented today
(logo, 3 bars, drag dots; hover grows upward to reveal timer, Pause, Stop).

## Architecture

```
 waveform webview (hidden)           Rust: recorder_pill            Native pill
 ───────────────────────────         ─────────────────────          ──────────────
 recorder://state ─────────────────▶ parse → PillState ───────────▶ update(state)
 tray://{pause,resume,stop}-recording ◀─┐                            │
 recorder://{retry-upload,              ├── map PillAction ◀─────────┘ on_action(a)
   continue-recording,                  │
   discard-recording}   ◀───────────────┘
                         create_library_window + recording://reveal  (body click)
```

- **`waveform` webview → headless recorder host.** `WaveformView` paints
  nothing and keeps the window click-through. (In PR 1 macOS launched it with
  the old `pillHidden=1` flag while Windows kept the Vue pill; PR 2 deletes the
  Vue pill, the flag and `recorder://pill-visible`.) The webview is still
  created visible (Windows' `getUserMedia` needs an on-screen window) and the
  watcher hides it as soon as capture is active, regardless of the Meetings
  window. `WaveformView` itself still re-shows it before a Resume restarts
  capture.
- **`src-tauri/src/recorder_pill/`** — `mod.rs` keeps the visibility watcher
  and gains the platform-neutral bridge: parsing `recorder://state` into a
  `PillState`, and mapping each `PillAction` to what the Vue pill used to do.
  `macos.rs` is the FFI to Swift; `win32.rs` is the Windows pill, built on the
  platform-free `layout.rs` (geometry, hit-testing, hover, drag, docking).
- **`src-tauri/recorder-pill/macos/`** — SwiftPM package `OatsRecorderPill`, a
  static library that `build.rs` compiles and links into the oats binary (plus
  search paths for the OS Swift runtime). An `NSPanel` (borderless,
  non-activating, floating level, all Spaces + over fullscreen apps, fixed
  size) hosts an `NSHostingView` of a SwiftUI view driven by an observable
  model; the capsule draws its own shadow.

## Windows (PR 2)

`win32.rs` draws the same pill with Win32 and Direct2D:

- Window: `WS_POPUP` with `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW |
  WS_EX_NOACTIVATE`, and `MA_NOACTIVATE` on `WM_MOUSEACTIVATE`, so it never
  takes focus or a taskbar button.
- Rendering: Direct2D/DirectWrite into a premultiplied 32-bpp DIB, presented
  with `UpdateLayeredWindow`. Clicks on alpha-0 pixels go to the window below,
  so the fixed-size panel has the same click-through as macOS. The shadow is
  stacked translucent rounded rects, since a DC render target has no blur.
- Input: hover from `WM_MOUSEMOVE` plus `TrackMouseEvent(TME_LEAVE)`. Buttons
  act on release over the same button. The body drags past a 3 px threshold
  (scaled to DPI) with `SetCapture`, and otherwise opens Meetings. A drag ends
  clamped to the monitor's work area.
- DPI: per-monitor. The DPI is re-read after every move, and
  `WM_DPICHANGED` is honored.
- Tooltips: a `tooltips_class32` control with one `TTF_SUBCLASS` tool per live
  button.
- Threading: all calls happen on Tauri's main thread. The state lives in a
  thread-local that is never borrowed across a call that can re-enter the
  window procedure.

## The C ABI (macOS)

All calls happen on the main thread; Rust dispatches with `run_on_main_thread`.

```c
typedef void (*oats_pill_action_cb)(int32_t action);
void oats_pill_create(oats_pill_action_cb on_action); // hidden panel; idempotent
void oats_pill_update(int32_t phase, bool paused, uint32_t duration_s,
                      const float *bars, uint32_t bar_count);
void oats_pill_set_visible(bool visible);             // show re-docks
void oats_pill_destroy(void);
```

- `phase`: 0 starting, 1 recording, 2 uploading, 3 success, 4 failed. A
  `closed` broadcast hides the pill; the `waveform` window's `Destroyed` event
  destroys it.
- `action`: 0 open meetings, 1 pause, 2 resume, 3 stop, 4 retry upload,
  5 continue recording, 6 discard recording.

| Action | Rust does | Vue handler |
|---|---|---|
| open meetings | `create_library_window`, then emit `recording://reveal` | — |
| pause / resume / stop | emit `tray://{pause,resume,stop}-recording` | existing |
| retry upload | emit `recorder://retry-upload` | `runFinalize` |
| continue recording | emit `recorder://continue-recording` | `resumeFailed` |
| discard recording | emit `recorder://discard-recording` | `dismissFailed` |

The three new events carry no payload and are only acted on while
`WaveformView` shows the failed state, like the buttons they replace.

## Interaction

- **Hover** expands the pill upward (bottom-anchored) to show the timer, Pause,
  and Stop; leaving collapses it. No expansion during uploading/success/failed.
  The panel itself never resizes. Resizing a window makes AppKit re-derive its
  tracking areas and report a resting pointer as exited, which made an
  earlier build flap open and shut. The panel is instead sized for the tallest
  capsule plus shadow room, and the capsule animates inside it. Hover comes
  from the pointer's position against the capsule's current rect, not the
  panel's rectangle; the expanded rect contains the collapsed one, so hover
  can't oscillate. Clicks on the panel's fully transparent pixels go to
  whatever is underneath (the window server routes by alpha).
- **Dropped near an edge**, a dragged pill is pulled back so even the tallest
  capsule fits on screen.
- **Click without movement** on the pill body opens Meetings on the recording.
- **Drag from anywhere** except a button: a press that moves more than 3pt
  hands off to the OS window drag (`NSWindow.performDrag`). This replaces the
  handle-only drag; the dots remain as the affordance.
- **First click acts** — `acceptsFirstMouse` plus a non-activating panel, so
  pressing Stop over another app's window works immediately and never pulls
  oats to the front.
- **States** — starting (flat bars), recording (yellow bars), paused (gray
  bars, Resume icon), uploading (spinner), success (green ✓), failed (red ✗ +
  Retry / Continue / Discard).
- **Labels** — every control has a tooltip and an accessibility label with the
  same strings as today ("Pause recording", "Stop and save recording", …).

## Testing

- **Rust:** `recorder://state` payload parsing, phase/action code mapping, the
  watcher's visibility decisions, and `layout.rs` (heights matching the Swift
  pill, element positions, hit-testing, hover, click-vs-drag, docking,
  clamping, the bar curve, the height animation and the logo path). These run
  on every host.
- **Swift (XCTest, `swift test`):** view-model layout per phase, timer
  formatting, bar clamping, the click-vs-drag threshold, and the dock frame
  math. Added to the macOS CI job.
- **Vitest:** `WaveformView` renders nothing and ignores the cursor. The
  recording flow is driven through the events the native pills send
  (`tray://stop-recording`, the three failed-upload events), and results are
  asserted on the broadcast phases.
- **Manual (macOS bundle):** record in cloud and local mode; minimize and close
  Meetings to reveal the pill; hover, click, drag, pause/resume, stop; force a
  failed upload (network off) and exercise Retry / Continue / Discard.
- **Windows (PR 2):** `win32.rs` is compile-checked locally against
  `x86_64-pc-windows-msvc` in a scratch crate, then built and tested in CI on
  `windows-latest`. A person validates it on a real Windows machine, including
  a mixed-DPI setup.

## Risks

- **Swift runtime linking.** `build.rs` adds the toolchain and SDK Swift library
  paths and an `/usr/lib/swift` rpath; the runtime ships in macOS 14.4+.
  Verified: the `cargo test` binary links and calls into Swift.
- **Hidden recorder webview.** Today the webview is already hidden for most of
  a recording (whenever Meetings is visible) with throttling disabled, so
  keeping it hidden for the whole recording is an existing, exercised state.
