---
name: oats-vue
description: Use when building or changing oats frontend UI — Vue 3 components, views, composables, routing, Tailwind, or the TipTap notes editor. Encodes this repo's frontend conventions.
---

# oats Frontend (Vue 3) Conventions

The frontend is Vue 3 + TypeScript + Vite, styled with Tailwind v4. Entry is
`src/main.ts`; the single root is `src/App.vue` (just `<RouterView/>`).

## Layout

- `src/main.ts` — app bootstrap **and the router**. Every window loads the same bundle
  and renders a route via `createWebHashHistory`. Add a view by importing it here and
  adding a `routes` entry. The router statically imports every view, so **one broken
  import breaks the whole app graph** (blank windows).
- `src/views/` — one `.vue` per screen (`SettingsView`, `LibraryView`,
  `MeetingDetailView`, `RecorderStrip`, `WaveformView`, …). Co-located `*.test.ts` use
  Vitest + `@vue/test-utils`. Pure helpers also live here as plain `.ts` + tests
  (e.g. `waveformBars.ts`, `meetingShareText.ts`).
- `src/composables/` — reusable reactive logic (`useBackend`, `useRecorder`,
  `useMeetingApi`, `usePendingUploads`, `useAutoRecord`, …). Prefer extracting logic
  into a composable with its own `*.test.ts` over fattening a view.
- `src/tauri.ts` — typed wrappers around backend `invoke` calls (`auth`, `api`,
  `getDesktopConfig`, …). **Call the backend through helpers here**, not raw `invoke`
  scattered in components. See `oats-tauri` for the invoke ↔ Rust contract.

## Conventions

- **`<script setup lang="ts">` + Composition API** everywhere. No Options API, no Vuex —
  state lives in composables and `@tauri-apps/plugin-store`.
- **Tailwind v4** via `@tailwindcss/vite` (config-light; utilities in templates). Global
  CSS in `src/assets/main.css`.
- **Routing is window-driven**: the Rust side opens a window pointed at a hash route
  (e.g. `/#/settings`, `/#/library`). A new screen = a new route here + a Rust window
  opener. See `oats-architecture` for the window topology.
- **Notes editor** uses TipTap (`@tiptap/vue-3`, StarterKit, task-list, typography,
  `tiptap-markdown`) — see `MeetingNotesEditor.vue` / `MeetingDetailView.vue`.
- **Icons**: `@heroicons/vue`.

## Testing

`npm test` runs Vitest (jsdom). Write a co-located `*.test.ts` for every new view,
component, or composable. Mock `@tauri-apps/api` invoke calls — don't hit the backend in
unit tests. Frontend tests are independent of the Rust suite.

When the change spans the Rust boundary, also read `oats-tauri`. For runtime debugging in
the real window, use `oats-debugging`.
