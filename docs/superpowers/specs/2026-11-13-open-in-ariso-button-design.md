# Open in Ariso Button — Design Spec

**Date:** 2026-11-13
**Issue:** #511
**Branch:** autopilot/issue-511

---

## Goal

Add an "Open in Ariso" button to the meeting detail (notes) page. Clicking it opens the
corresponding Ariso web meeting-notes page for the current meeting in the system's
default browser.

## Non-Goals

- Local (offline) recordings: there is no Ariso counterpart for a local-only recording,
  so the button does not apply there.
- Any change to the share flow (`ShareMeetingPopover.vue`) — this is a separate,
  always-available link to the user's own canonical meeting page, not a share link.
- Any backend/sidecar change.
- Any capability change — `openUrl` from `@tauri-apps/plugin-opener` already has
  `https://*` in its default scope (`opener:default` → `allow-default-urls`), confirmed
  in `tauri-plugin-opener`'s own `permissions/allow-default-urls.toml`, so no new
  `src-tauri/capabilities/*.json` entry is needed.

## Constraints

- Cloud-only feature: gated on `!detail.isLocal`, the same gate `canShare`,
  `ShareMeetingPopover`, and `canAssignSpeakers` already use for cloud-specific UI in
  `MeetingDetailView.vue`.
- `src-tauri/src/capabilities/` (i.e. `src-tauri/capabilities/`), `package.json`,
  `package-lock.json`, and `src-tauri/Cargo.lock` cannot be changed, and this feature
  does not need to touch any of them (see Non-Goals).
- The web app base URL must come from `getDesktopConfig().webAppBaseUrl` (already used by
  `ShareMeetingPopover.vue`), not a hardcoded host — the base URL differs between
  prod/dev builds (`DEFAULT_WEB_APP_BASE_URL` in `src-tauri/src/commands.rs` is
  `https://web.ari.ariso.ai` in release, `https://web-dev.ari.ariso.ai` in debug).

---

## Decisions

| # | Question | Answer | Source |
|---|----------|--------|--------|
| 1 | The issue's URL path is `/meeting-ntoes/{meeting-id}` — typo or real path? | Typo. Use `/meeting-notes/{meeting-id}`. Every other reference in this codebase (`useMeetingApi.ts`, `ShareMeetingPopover.vue`'s own link builder, `tray_meeting.rs`, `commands.rs`) spells it `meeting-notes`; `meeting-ntoes` appears nowhere | Default |
| 2 | Hardcode `https://web.ari.ariso.ai`, or derive the host? | Derive it from `getDesktopConfig().webAppBaseUrl`, mirroring `ShareMeetingPopover.vue`'s `existingShareUrl` computed, so the link stays correct in dev builds and if the web host ever changes | Default |
| 3 | Where should the button appear? | In the header's `.head-actions` row, to the left of the existing Share button (order: Open in Ariso → Share → Delete → Close) | Default |
| 4 | Should it render only for cloud meetings? | Yes — gate on `!detail.isLocal`, matching the existing `canShare`/`canAssignSpeakers` pattern and the issue's own implied scope (a meeting-notes URL only exists for Ariso meetings) | Default |
| 5 | What ID goes in the URL — the meeting ID or the share short code? | The meeting ID (`detail.id`), matching the issue's "Replace `{meeting-id}` with the current meeting's ID" and the `/meeting-notes/:id` API convention used throughout `useMeetingApi.ts`. (The share short code is a different, shareable identifier used only by `ShareMeetingPopover`.) | Default |

---

## Interfaces

No new backend commands or composables. One new button + handler in
`src/views/MeetingDetailView.vue`, reusing:

- `getDesktopConfig()` from `../tauri` (already imported pattern in
  `ShareMeetingPopover.vue`; `MeetingDetailView.vue` will add its own import).
- `openUrl` from `@tauri-apps/plugin-opener` (already used in
  `useRecordingPermissions.ts`, `useMeetingNotifications.ts`).

### New computed / ref / function (`MeetingDetailView.vue`)

```typescript
const webAppBaseUrl = ref('');

const canOpenInAriso = computed(() => !!detail.value && !detail.value.isLocal);

async function onOpenInArisoClick(): Promise<void> {
  const d = detail.value;
  if (!d || !webAppBaseUrl.value) return;
  await openUrl(`${webAppBaseUrl.value}/meeting-notes/${encodeURIComponent(d.id)}`);
}
```

`webAppBaseUrl` is populated in the existing top-level `onMounted` hook (the one that
currently only sets `nativeShareSupported`), via `getDesktopConfig()`, matching
`ShareMeetingPopover.vue`'s try/catch-and-stay-empty pattern — a failed fetch leaves the
button absent rather than broken.

### Template addition (`.head-actions`, before the Share button)

```html
<button
  v-if="canOpenInAriso"
  class="btn-open-ariso"
  type="button"
  title="Open in Ariso"
  @click="onOpenInArisoClick"
>
  <svg viewBox="0 0 24 24" class="ic"><path d="M14 4h6v6" /><path d="M20 4L10 14" /><path d="M18 13v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h6" /></svg>
  Open in Ariso
</button>
```

### CSS addition

`.btn-open-ariso` reuses the exact `.btn-share` box model (same pill, border, shadow),
just as `.note-del-btn` already does — add it as a shared selector:

```css
.btn-share,
.btn-open-ariso {
  display: flex; align-items: center; gap: 6px;
  height: 32px; padding: 0 12px;
  background: #fff; border: 1px solid #d6d6d6; border-radius: 8px;
  box-shadow: 2px 2px 0 #e7e5e2;
  font-family: inherit; font-size: 14px; font-weight: 600; color: #1a1a1a; cursor: pointer;
}
.btn-share:hover,
.btn-open-ariso:hover { background: #fbfbfb; }
```

(Replaces the current standalone `.btn-share` rule + hover rule.)

---

## Acceptance Criteria

- [ ] The meeting notes page displays a clearly labeled "Open in Ariso" button (issue
      AC1).
- [ ] Clicking the button opens `https://web.ari.ariso.ai/meeting-notes/{meeting-id}`
      (or the dev-build equivalent host) for the current meeting in the system browser
      (issue AC2).
- [ ] The correct meeting ID (`detail.id`) is used in the URL (issue AC3).
- [ ] The button is not shown for local (offline) recordings.
- [ ] The button is not shown if `getDesktopConfig()` fails before the user clicks (no
      base URL to build a link from).
- [ ] `npm test` passes.
- [ ] `npm run vite:build` succeeds.
- [ ] `cargo build --locked --manifest-path src-tauri/Cargo.toml` succeeds.

---

## Test Strategy

### Frontend unit tests (`MeetingDetailView.test.ts`)

- Button renders with text "Open in Ariso" for a cloud (`isLocal: false`) meeting detail.
- Button is absent for a local (`isLocal: true`) meeting detail.
- Clicking the button calls `openUrl` (mock `@tauri-apps/plugin-opener`) with
  `${webAppBaseUrl}/meeting-notes/${id}`, using a mocked `getDesktopConfig()` resolving to
  a known `webAppBaseUrl`.
- Button does not throw / stays absent-of-crash when `getDesktopConfig()` rejects.

### Cannot verify at this layer

- That the opened URL actually loads the right page in a real browser against the real
  Ariso web app (requires a real account and the live web app).
- Visual placement/spacing in the running desktop app (requires the running app; `npm
  run tauri:dev` was not exercised in this run).
