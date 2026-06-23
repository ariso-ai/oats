# Silence-Detection Stop Prompt

**Date:** 2026-06-17
**Branch:** `fix/ux-current-recording-status`
**Status:** Approved design, pending implementation plan

## Problem

Today a recording silently auto-stops after 15 minutes of no captured sound
(`silenceWatch.ts` → `WaveformView.vue:497-509`). The stop is invisible and
non-consentful: a user who stepped away, or whose meeting genuinely continued
in low volume, gets cut off with no warning and no chance to keep going.

## Goal

Replace the silent stop with a **visible, consentful** flow:

1. After **10 minutes** of silence, fire a **native OS notification** offering
   *Keep recording* / *Stop now*.
2. If the user does not respond within **60 seconds**, **auto-stop**.
3. *Keep recording* resets the silence timer; if silence continues for another
   10 minutes, prompt again (repeats indefinitely).
4. *Stop now* stops immediately.

The auto-stop must remain robust even when the notification cannot be delivered
(unsigned dev build, notifications disabled): the 60-second grace timer is
frontend-owned, so it stops the recording regardless of notification delivery.

## Non-Goals

- No VAD/ML — silence stays threshold-based on audio peak (`SILENCE_LEVEL = 300`
  in `useRecorder.ts`), unchanged.
- No new settings/UI to configure the thresholds (hard-coded constants for now).
- No change to pause semantics — a paused recording is never prompted or stopped.

## Architecture

Silence detection already lives in the **frontend** (`useRecorder.ts` tracks
`lastSoundAt` from per-frame audio peaks; `WaveformView.vue` runs a 1-second
monitor loop). Native action-button notifications already live in **Rust**
(`meeting_notifications.rs`, `prompt_auto_record`). We keep that split:

- **Frontend owns the state machine and all timing.** It is the source of truth
  for `lastSoundAt`, is unit-testable via pure functions, and — critically — can
  **cancel a pending stop if audio resumes during the 60s grace window**. A
  Rust-owned timeout (like `prompt_auto_record`'s) could not observe resumed
  audio and would wrongly stop a meeting that just came back.
- **Rust only renders the notification and forwards clicks.** A
  `show_silence_prompt` command displays the native notification; button taps
  emit `silence-prompt://keep` / `silence-prompt://stop`; `dismiss_silence_prompt`
  clears it.

## Frontend State Machine

Driven by the existing 1-second interval in `WaveformView.vue` (the loop that
currently calls `shouldAutoStop`). Two states: `idle` and `prompted`.

Guard (all ticks): skip while `isUploading`, `uploadResult` set, or not recording.

- **idle → prompted**: `shouldPromptSilence(lastSoundAt, now, paused)` is true
  (i.e. `!paused && now - lastSoundAt >= SILENCE_PROMPT_MS`).
  Action: `invoke('show_silence_prompt')`; record `promptShownAt = now`.
- **prompted, each tick**:
  - **audio resumed** (`lastSoundAt > promptShownAt`) → `invoke('dismiss_silence_prompt')`, → **idle**.
  - **paused** → `invoke('dismiss_silence_prompt')`, → **idle** (pause freezes everything, as today).
  - **grace elapsed**: `shouldAutoStopAfterPrompt(promptShownAt, lastSoundAt, now, paused)`
    is true (i.e. `!paused && now - promptShownAt >= SILENCE_GRACE_MS`) → `handleStop()`.
- **`silence-prompt://keep` event** → seed `lastSoundAt = now`,
  `invoke('dismiss_silence_prompt')`, → **idle** (natural re-prompt after another 10 min).
- **`silence-prompt://stop` event** → `handleStop()`.

Notes:
- `promptShownAt` is a `let` in `WaveformView.vue` (or a module ref), reset to
  `null` whenever returning to **idle**.
- The two new event listeners are registered in `onMounted` and torn down in
  `onUnmounted`, alongside the existing `tray://*` / `auto-record://stop` listeners.
- On *Keep recording*, seeding `lastSoundAt = now` reuses the exact mechanism the
  resume path already uses, so re-prompting is automatic.

## Constants & Pure Helpers (`silenceWatch.ts`)

```ts
export const SILENCE_PROMPT_MS = 10 * 60_000; // 10 min of silence → prompt
export const SILENCE_GRACE_MS = 60_000;       // 60 s grace → auto-stop

export function shouldPromptSilence(
  lastSoundAt: number, now: number, paused: boolean,
): boolean {
  if (paused) return false;
  return now - lastSoundAt >= SILENCE_PROMPT_MS;
}

export function shouldAutoStopAfterPrompt(
  promptShownAt: number, lastSoundAt: number, now: number, paused: boolean,
): boolean {
  if (paused) return false;
  if (lastSoundAt > promptShownAt) return false; // audio resumed → cancel
  return now - promptShownAt >= SILENCE_GRACE_MS;
}
```

The existing `shouldAutoStop` / `SILENCE_TIMEOUT_MS` (15 min) and its single call
site in `WaveformView.vue:497-509` are removed and replaced by the state machine
above. Remove `shouldAutoStop` if it has no other callers (verify during impl).

## Rust Changes

In `meeting_notifications.rs` (mirroring the `AUTO_RECORD_*` category/action
pattern at ~lines 491-629) plus command registration in `main.rs`:

- **Category + actions**: e.g. `SILENCE_PROMPT_CATEGORY`,
  `SILENCE_KEEP_ACTION`, `SILENCE_STOP_ACTION`. Register the category with
  `UNUserNotificationCenter` next to the existing auto-record category.
- **`show_silence_prompt(app)`** Tauri command:
  - Signed macOS bundle → native `UNUserNotificationCenter` notification titled
    e.g. *"Still recording"* / body *"No audio for 10 minutes. Keep recording or
    stop?"* with the two action buttons, using a fixed identifier (e.g.
    `"silence-prompt"`) so it can be dismissed.
  - Dev/unsigned/non-macOS → plain `tauri-plugin-notification` banner (no
    buttons). Frontend grace timer still auto-stops after 60 s — acceptable
    degradation.
- **`dismiss_silence_prompt(app)`** Tauri command: remove delivered + pending
  notifications for the fixed identifier
  (`removeDeliveredNotifications` / `removePendingNotificationRequests`).
- **Delegate**: extend the existing `did_receive` handler so that the silence
  category's actions `emit` `silence-prompt://keep` / `silence-prompt://stop`.
  `will_present` keeps showing the banner when the app is frontmost.
- Register both commands in the `invoke_handler` in `main.rs`.

## Events & Commands Summary

| Direction | Name | Payload | Purpose |
|---|---|---|---|
| FE → BE (invoke) | `show_silence_prompt` | none | Display the native silence notification |
| FE → BE (invoke) | `dismiss_silence_prompt` | none | Clear the notification |
| BE → FE (emit) | `silence-prompt://keep` | none | User chose *Keep recording* |
| BE → FE (emit) | `silence-prompt://stop` | none | User chose *Stop now* |

## Testing

- **Unit (Vitest)** alongside existing `silenceWatch` tests:
  - `shouldPromptSilence`: fires at/after 10 min; not before; never while paused.
  - `shouldAutoStopAfterPrompt`: fires at/after 60 s past prompt; not before;
    cancels when `lastSoundAt > promptShownAt`; never while paused.
- **Manual** (per macOS-permission-testing memory): exercise the notification +
  both buttons + 60 s timeout on the **signed bundle** (`ai.ariso.desktop`), not
  the adhoc dev binary. Verify: prompt at 10 min; *Stop now* stops; *Keep
  recording* continues and re-prompts after another 10 min; ignoring it
  auto-stops at ~11 min; making noise during the grace window cancels the stop.

## Risks / Edge Cases

- **Notification undeliverable** (permissions off, unsigned build): covered —
  frontend grace timer auto-stops regardless.
- **Audio resumes during grace**: covered — `shouldAutoStopAfterPrompt` returns
  false once `lastSoundAt > promptShownAt`; tick dismisses and returns to idle.
- **Pause during prompt**: covered — tick dismisses and returns to idle.
- **Stale notification after recording ends some other way** (manual stop while
  prompted): `handleStop` / `onUnmounted` should `dismiss_silence_prompt` so no
  orphaned notification lingers.
