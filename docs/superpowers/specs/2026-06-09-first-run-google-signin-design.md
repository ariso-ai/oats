# First-Run Onboarding (Google Sign-In) — Design

**Date:** 2026-06-09
**Status:** Approved (design), pending implementation plan

## Goal

On the very first launch of the app, show a first-run onboarding window. Its
first (and currently only) step is a "Sign in with Google" screen with a
**Skip** button. The window is built to host **additional onboarding steps
later** without structural change. After the user finishes the flow (signs in or
skips through to the end), persist a flag and never show onboarding automatically
again. The user can still sign in later from Settings.

## Context

Ariso is a Tauri (Rust) + Vue 3 tray/menubar app. It has **no main window** on
launch: `setup()` creates a hidden `main` bootstrap window (a permanent,
invisible JS worker that drives meeting-notification syncing) and a pre-created
hidden `settings` window shown on demand from the tray. Windows are created at
runtime via `WebviewWindowBuilder`; routes are hash routes (`/#/settings`,
`/#/library`, …) registered in `src/main.ts`.

Google sign-in already exists end-to-end:
- `src/tauri.ts` → `auth.googleSignIn()`, `auth.checkSession()`, `auth.signOut()`
- `src/views/SettingsView.vue` already renders a "Sign in with Google" button
  (the `.google-btn` markup + SVG) and a `handleGoogleSignIn()` handler that, on
  success, calls `emitNotificationsSync()`.

Settings persist in `settings.json` via `@tauri-apps/plugin-store`, read/written
from JS (e.g. `getBackendSetting`/`setBackendSetting` in `src/tauri.ts`).

## Decisions

- **Trigger:** Only the very first launch ever. A persisted `onboarded` boolean
  in `settings.json` is the single source of truth. Once `true` onboarding never
  auto-shows again.
- **Dedicated window:** A separate `onboarding` window (label `"onboarding"`),
  not the `main` worker window. Onboarding is a first-run-only, transient UI;
  keeping it in its own window means closing it is harmless and the permanent
  notification worker is never coupled to it.
- **Steps live inside the window:** The onboarding view owns its steps via an
  internal `currentStep` state — **not** vue-router routes and **not** extra
  windows. Today there is one step (`signin`); future steps are appended to a
  list with no new window/route plumbing.
- **`onboarded` flag is set once, at the end of the flow** (after the last step),
  not per-step. "Onboarded" means "finished the first-run flow."
- **Skip behavior:** Skip advances past the current step. With a single step it
  finishes the flow (sets `onboarded:true`, closes the window). The app continues
  in the tray.
- **Visual style:** Reuse the existing Settings card + `.google-btn` styling for
  consistency and minimal new CSS.
- **First-run check location (Approach A):** JS-driven. The hidden `main`
  bootstrap window checks the flag on launch and asks Rust to open the onboarding
  window. This keeps store reads in JS (where all other reads live) and makes the
  Rust addition a near-copy of the existing `create_settings_window` command.

## Architecture

A dedicated first-run onboarding window, shown once ever, gated by the
`onboarded` flag in `settings.json`, hosting an internal step sequence.

### Components / changes

1. **`src/views/OnboardingView.vue`** (new) — route `/#/onboarding`. The
   step host. Reuses the Settings card + `.google-btn` styling.
   - Internal step model: a `steps` list (currently `['signin']`) and a
     `currentStep` ref. Rendering switches on the current step.
   - `advance()`: if there is a next step, increment `currentStep`; otherwise call
     `finishOnboarding()`.
   - `finishOnboarding()`: `setOnboarded(true)`, then `getCurrentWindow().close()`.
   - **Sign-in step UI:** Ariso logo + short heading (e.g. "Welcome to Ariso"),
     "Sign in with Google" button (same SVG/markup as Settings), and a subtle
     "Skip" text button.
     - On **sign-in success**: fire `emitNotificationsSync()` (matching Settings),
       then `advance()`.
     - On **Skip**: `advance()`.
     - On **sign-in error**: show the inline error, stay on the step.

2. **`src/main.ts`** — register
   `{ path: '/onboarding', name: 'Onboarding', component: OnboardingView }`.

3. **`src-tauri/src/commands.rs`** — new `create_onboarding_window` command, a
   near-copy of `create_settings_window`: focus-if-exists guard,
   `WebviewUrl::App("/#/onboarding")`, centered, sized ~450×600, `resizable(false)`.
   Registered in `main.rs`'s `generate_handler!` list. (No close-interceptor
   needed — closing the onboarding window is harmless; it is not a worker.)

4. **`src/views/BootstrapView.vue`** — in `onMounted`, after the existing
   notification setup, read `onboarded` from `settings.json`; if falsy,
   `invoke('create_onboarding_window')`. The flag is the single source of truth —
   no session check needed (a first launch never has a session).

5. **`src/tauri.ts`** — small helpers matching the existing
   `getBackendSetting`/`setBackendSetting` style: `isOnboarded()`,
   `setOnboarded()`, and `openOnboardingWindow()`.

### Data flow

```
App launch → hidden "main" window boots → BootstrapView.onMounted
   → settings.json.onboarded ?
        true  → do nothing (normal tray operation)
        false → invoke create_onboarding_window → visible "onboarding" window @ /#/onboarding
                   → step "signin":
                        "Sign in with Google" → auth.googleSignIn()
                              success → emitNotificationsSync() → advance()
                              error   → show message, stay on step
                        "Skip" → advance()
                   → advance(): more steps ? currentStep++ : finishOnboarding()
                   → finishOnboarding(): setOnboarded(true) → window.close()
```

### Adding a step later (illustrative, not in scope now)

Append a descriptor to `steps` (e.g. `'permissions'`) and add its render branch
in `OnboardingView`. `advance()`/`finishOnboarding()` and the `onboarded` flag
are unchanged: the new step slots between sign-in and finish with no window,
route, or Rust changes.

## Error handling

- **Sign-in failure:** show the inline error message (same pattern as Settings),
  stay on the sign-in step so the user can retry or Skip. The `onboarded` flag
  stays unset until the flow finishes.
- **`create_onboarding_window` invoke fails:** log and continue — the app still
  works from the tray.

## Testing

`src/views/OnboardingView.test.ts` (vitest, mocking `tauri.ts` as
`SettingsView.download.test.ts` does):
- renders the sign-in step with both the Google button and the Skip button;
- **Skip** (single-step config) sets `onboarded:true` and closes the window;
- **sign-in success** emits notifications sync, then (single-step) sets the flag
  and closes;
- **sign-in error** stays on the step and shows the message;
- `advance()` with a multi-step config moves to the next step instead of closing
  (guards the future-steps contract).

## Design note — avoiding over-engineering

- The Google sign-in glue is now used in two places (Settings + Onboarding) but
  differs in outcome (Settings updates in-page state; Onboarding advances the
  flow). The two handlers stay separate rather than being hoisted into a shared
  composable — the divergence makes a shared abstraction more awkward than the ~8
  duplicated lines. Revisit only if a third consumer appears.
- The step model is deliberately minimal (a list + an index + `advance()`). No
  step registry, router child-routes, or per-step component framework until a
  real second step justifies it.

## Out of scope

- Changing the existing Settings sign-in flow or the OAuth implementation.
- Re-showing onboarding on later launches, session-expiry prompts.
- Implementing any onboarding step beyond the sign-in screen (the step model
  exists to make later steps cheap, but none are built now).
