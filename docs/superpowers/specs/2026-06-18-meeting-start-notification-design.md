# Meeting-start notification — design

**Issue:** [#121](https://github.com/ariso-ai/oats/issues/121)
**Date:** 2026-06-18
**Status:** Approved (brainstorming) — ready for implementation plan

## Problem

The meeting-start notification (the auto-record prompt shown when the mic monitor
detects a live meeting) should feel cleaner and faster to act on — closer to the
Zoom meeting-start pattern. Issue #121 asks for: the app icon + title + subtitle, a
primary **Take notes** button, a **countdown bar at the top** of the banner, and a
chevron (⌄) that discloses secondary actions such as **Dismiss**.

A real macOS notification (`UNUserNotificationCenter`) **cannot** render a custom
countdown bar — macOS has no custom notification UI (content extensions are iOS-only).
So the countdown banner is built as a **custom borderless window** instead, the way
Zoom does it.

## Scope

- **In scope:** Replace the auto-record prompt UI only — today's "Meeting detected /
  Start recording this meeting?" UNC banner with Record/Dismiss buttons
  (`meeting_notifications.rs`, the `prompt_auto_record` path).
- **Out of scope:** The meeting-prep "ready" notifications keep using plain UNC
  banners (and their deep-link click handling) unchanged.

## Decisions (from brainstorming)

- **Visual:** "Native macOS mimic" — one compact row: app icon · title/subtitle ·
  `Take notes ⌄`. Thin countdown bar at the very top, draining full→empty over the
  timeout, macOS-blue fill. Respects system light/dark appearance.
- **Expiry behavior (unchanged):** when the bar reaches zero with no click, apply the
  existing setting-driven default — auto-record **ON** → start taking notes; auto-record
  **OFF** → dismiss. This reuses the `default_record` value already passed to
  `prompt_auto_record`.
- **Implementation:** a custom Tauri webview window + Vue route (not UNC, not native
  AppKit). Works in `tauri dev` and unsigned builds; no code-signing required.

## Architecture

A new window mirroring the waveform-pill pattern (`commands.rs:573`):

- `label: "meeting-prompt"`, `WebviewUrl::App("/#/meeting-prompt?...")`.
- `decorations(false)`, `transparent(true)`, `always_on_top(true)`,
  `skip_taskbar(true)`, `resizable(false)`, `shadow(false)`.
- `focused(false)` — must **never steal focus** from the live meeting.
- Inner size ≈ `360×84` (compact A layout; final value tuned during implementation).
- Position: **top-right** of the primary monitor with a ~16px margin (the macOS
  notification corner), computed from `primary_monitor()` like the pill's edge dock.
- Single instance: only one prompt is ever live per meeting (the mic-monitor state
  machine stays in its Recording phase), so if a `meeting-prompt` window already
  exists it is closed/replaced before opening a new one.

## Data flow (reuses existing plumbing)

Rust remains the single source of truth for timing; the CSS bar is cosmetic only.

1. `mic_monitor.rs` calls `prompt_auto_record(app, default_record)` as today.
2. `prompt_auto_record` keeps its `oneshot` channel and the 10s
   `AUTO_RECORD_PROMPT_TIMEOUT`. Instead of posting a UNC banner it opens the
   `meeting-prompt` window, passing the title, subtitle, and timeout (seconds) via URL
   query params.
3. The Vue view (`MeetingPromptView.vue`) renders icon/title/subtitle, a countdown bar
   animating over the passed duration, and a `Take notes ⌄` control whose chevron opens a
   small menu containing **Dismiss**.
4. On click, the view calls a new invoke command `resolve_meeting_prompt(record: bool)`,
   which calls `deliver_auto_record_decision(record)` to fill the `oneshot`; Rust then
   closes the window.
5. On timeout (no click), Rust closes the window and returns `default_record` — the bar
   simply finishes draining at the same moment.

## Copy

Generic, because the mic monitor detects meetings from audio and usually has no
calendar meeting name:

- Title: **"Meeting started"**
- Subtitle: **"oats can take notes for you."**

(If a meeting name is readily available later it can be substituted, but that is not
required for this change.)

## Cleanup of now-dead UNC code (in scope)

The custom window fully replaces the UNC auto-record prompt, so remove:

- Constants `AUTO_RECORD_CATEGORY`, `AUTO_RECORD_ACTION_RECORD`,
  `AUTO_RECORD_ACTION_DISMISS`.
- `register_auto_record_category` and its call in `macos_un::init`.
- `macos_un::show_auto_record` and `show_auto_record_prompt`.
- The two auto-record action branches in the delegate's `did_receive`.

The `UNUserNotificationCenter` delegate **stays** — it still opens meeting-prep
deep links on click. The meeting-prep `show()` path is untouched.

## Security

- `resolve_meeting_prompt(record: bool)` is a new invoke command and is added to the
  capabilities allowlist. It takes a single boolean — no path, URL, or other untrusted
  input — so it adds no new attack surface (per `oats-security`).
- The new window loads only a local app route; no external/remote URL.

## Error handling & edges

- **Window-create failure:** log and let `prompt_auto_record` resolve on the timeout
  default — no worse than today's UNC failure path.
- **Focus:** `focused(false)` keeps the meeting uninterrupted.
- **No decorations:** the only ways out are the two buttons or the timeout.
- **Notifications disabled:** unchanged — still short-circuited upstream before any
  prompt is shown.

## Testing

- **Frontend (Vitest):** `MeetingPromptView` renders title/subtitle; **Take notes**
  invokes `resolve_meeting_prompt(true)`; the chevron's **Dismiss** invokes
  `resolve_meeting_prompt(false)`; the bar animates over the passed duration.
- **Rust:** the timeout/default contract of `prompt_auto_record` is preserved; existing
  behavior. Window positioning is verified manually.
- **Manual:** drive the built bundle / running app via the `oats-desktop` MCP
  (`oats-debugging`) to confirm placement, the drain animation, both button paths, and
  the timeout default in both auto-record modes.

## Out of scope / non-goals

- Hover-to-pause the countdown (macOS-style) — omitted (YAGNI); can be added later.
- Showing the specific calendar meeting name.
- Any change to meeting-prep notifications.
