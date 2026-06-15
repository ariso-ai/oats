# Meeting Sharing in the Desktop Detail Panel — Design

**Date:** 2026-06-14
**Status:** Approved (pending spec review)

## Goal

Make the **Share** button in `MeetingDetailView.vue`'s header fully functional,
with backend-specific behaviour:

- **Ariso backend:** port the full meeting-sharing feature from the web app's
  `apps/web-ui/src/Pages/MeetingNotesDetail.vue` (reference) — the share dropdown
  (visibility levels, public-link expiry, copy link, email invites, shared-
  participant avatars), adapted to the desktop's environment and visual language.
- **Local backend:** open the **native macOS share sheet**
  (`NSSharingServicePicker`) over the recording's notes, giving Copy / AirDrop /
  Mail / Messages / Notes / etc.

The Share button is shown for **both** backends; its click handler branches on
`detail.isLocal`.

## Scope

**Ariso backend — in scope (full parity with the reference share dropdown):**

- Visibility levels: `private` / `workspace` / `public`, chosen via a sub-menu.
- Public-link expiry picker: free text 1–365 days with presets (7/14/30/60/90),
  default 30; shows "Currently public until {date}" when already public; requires
  an explicit Save.
- Copy link (workspace/public only) with transient "Copied!" feedback.
- Email invites: send to an address, list already-shared emails, unshare.
- Shared-participant avatar grid: ring/colour indicates who the notes are shared
  with; clicking a shared avatar starts an unshare flow. Extra shared emails (not
  in the participant list) render as their own tiles.
- Permission gating that matches the web exactly.

**Local backend — in scope (native macOS share):**

- Clicking Share composes a single markdown document (meeting title + date heading,
  then an **AI Notes** section, then a **My Notes** / personal-note section) and
  opens the native macOS share sheet anchored to the button. Empty sections are
  omitted.

**Out of scope (not requested):** Google-Docs / markdown export, delete-meeting,
attendees-edit modal, coaching drawer, transcript export. Native sharing is
macOS-only (the app is macOS-only).

## Decisions (confirmed with user)

1. **Full parity** for the Ariso share feature.
2. **Remove** the existing standalone header copy-link icon button; copy-link lives
   inside the Ariso share dropdown (matching the reference).
3. **Match the web's permission gating exactly** (Ariso).
4. **Local share content:** both notes (AI + personal) under a title/date heading,
   as one markdown/text document.
5. **Local share UI:** native macOS share sheet (`NSSharingServicePicker`) over
   text, anchored to the Share button.

## Architecture (Approach B: dedicated popover component)

The codebase already has two clean seams:

- `Backend` (`useBackend.ts`) — abstracts **local-vs-ariso storage** (list, detail,
  transcript, individual note, rename).
- `useMeetingApi.ts` — the **HTTP layer** (`api.request(method, path, body)` →
  `{ status, data }`; no axios interceptors, no `params`/`suppressGlobalError`).

The two share modes are distinct features sharing one button, not one feature with
two backends:

- **Ariso link/email sharing** is server-side with no local equivalent, so its
  write/HTTP calls live in `useMeetingApi` and are called directly by the popover
  (not threaded through the `Backend` interface, which would force `LocalBackend`
  to stub Ariso-only methods). The *read* path for gating fields still flows
  through `Backend.getMeetingDetail` so the detail panel has one load path.
- **Local native sharing** is an OS feature with no server equivalent, handled by a
  Rust command + a pure text-composition helper, invoked directly from the view.

Neither belongs on the `Backend` interface; the view branches on `detail.isLocal`.

### Components / units

1. **`useMeetingApi.ts` — new HTTP methods** (same endpoints the web uses):
   - `shareMeeting(meetingId, visibility, expiresInDays?)`
     → `POST /meeting-notes/:id/share` body `{ visibility, expiresInDays? }`
     → returns `{ shareUrl: string; shortCode?: string; publicShareExpiresAt: string | null }`
     (read from `data.shareUrl`, `data.shortCode`, `data.publicShareExpiresAt`).
   - `listShareEmails(meetingId)`
     → `GET /meeting-notes/:id/share-emails` → returns `string[]`
     (from `data.items[].email`); errors resolve to `[]` (non-critical).
   - `sendShareEmail(meetingId, email)`
     → `POST /meeting-notes/:id/share-email` body `{ email }`
     → returns `{ alreadyShared: boolean }` (from `data.already_shared`).
     Surfaces the server's `data.error` message to the caller on failure.
   - `unshareEmail(meetingId, email)`
     → `DELETE /meeting-notes/:id/share-email?email=<encoded>` (query param appended
     to the path since `api.request` takes no params object).
   - All use `assertOk` for the success-status checks, consistent with the file.

2. **`useMeetingApi.ts` / `useBackend.ts` — extend the read types** to surface the
   share-gating fields the `/meeting-notes/:id` payload already returns:
   - `MeetingNotes` gains: `short_code?`, `public_share_expires_at?`,
     `shareMeetingNotesToPublic?` (`'attendee_and_host' | 'host_only' | 'off'`), and
     participant `id?` (on `MeetingNotesParticipant`).
   - `MeetingParticipantInfo` gains `id?: number` (avatarUrl already exists).
   - `MeetingDetail` gains: `shortCode?`, `publicShareExpiresAt?: string | null`,
     `shareMeetingNotesToPublic?`.
   - `ArisoBackend.getMeetingDetail` maps these through; `LocalBackend` leaves them
     undefined.

3. **`ShareMeetingPopover.vue` — new component.** Props: `{ detail: MeetingDetail;
   meetingId: string }`. Owns all share state and UI, styled with scoped CSS
   matching `MeetingDetailView` (white card, `#e5e6e3` borders, `2px 2px 0` shadow,
   Polymath font — **not** Tailwind). Responsibilities:
   - On mount: fetch `webAppBaseUrl` via `getDesktopConfig()` (cached in the
     component) and load `listShareEmails`.
   - Derive `isHost` = `participants.some(p => p.role === 'host' && p.self)` and
     `isAttendee` = `participants.some(p => p.role !== 'host' && p.self)` from
     `detail.participants` (matching the reference).
   - Derive `canSharePublic` from `detail.shareMeetingNotesToPublic` + role
     (`off` → false; `attendee_and_host` → host||attendee; `host_only` → host).
   - Avatar grid, email invite row, visibility sub-menu, public-expiry picker,
     copy-link — full parity with the reference template, restyled.
   - On successful share/unshare, mutate the passed `detail` object's
     `visibility` / `shortCode` / `publicShareExpiresAt` directly. This is
     consistent with how `MeetingDetailView` already mutates `detail.title` and
     `detail.hasIndividualNote`. (Vue object props are reactive references; the
     parent sees the change, so reopening the popover shows fresh state.)
   - Emits `close` so the parent can dismiss it.

4. **`src/views/meetingShareText.ts` — new pure module (local share).**
   `composeLocalShareText(detail, personalNote)` builds the markdown document:
   `# {title}` / formatted date / `## AI Notes` + `detail.note` / `## My Notes` +
   personal note. Empty sections omitted; returns `''` when nothing to share.
   Pure and unit-testable (mirrors the existing `waveformBars.ts` /
   `recordingSettings.ts` split-from-view pattern).

5. **Rust — `commands::share_text_native` (macOS) + `tauri.ts` binding.**
   - Signature: `share_text_native(text: String, anchor: { x, y, width, height })`.
   - Uses `objc2` + a new `objc2-app-kit` dependency to build an
     `NSSharingServicePicker` with the text as an `NSString` item and call
     `showRelativeToRect:ofView:preferredEdge:` on the focused window's content
     view (main thread). The `anchor` rect (CSS px from the button's
     `getBoundingClientRect()`) is converted to the content view's coordinate
     system (flip Y: `appkitY = viewHeight - (y + height)`); CSS px ≈ AppKit
     points, so no DPR scaling.
   - Registered in `main.rs`'s `generate_handler!`. Custom app commands need no
     capability entry (confirmed: existing `commands::*` aren't in
     `capabilities/default.json`). Non-macOS build: compile-time `cfg` no-op that
     returns an "unsupported" error.
   - `tauri.ts` exposes `shareTextNative(text, anchor)` calling
     `invoke('share_text_native', …)`.

6. **`MeetingDetailView.vue` — wiring:**
   - Remove the stub Share button's no-op and the standalone copy-link icon button.
   - Render the Share button for **both** backends:
     - Local recordings: always show (it shares note content).
     - Ariso: show only when `isHost || isAttendee`.
   - Click handler branches on `detail.isLocal`:
     - **Ariso** → toggle `showShare`, rendering `<ShareMeetingPopover
       :detail="detail" :meeting-id="detail.id" @close="showShare = false" />`
       anchored under the header.
     - **Local** → load the personal note (`notesPersistence.load(item)`), call
       `composeLocalShareText`, then `shareTextNative(text, buttonRect)`. No-op
       (or inline message) when the composed text is empty.
   - Keep the Close button.

## Desktop adaptations (where the web can't be ported verbatim)

- **Share URL base.** The web builds `existingShareUrl` from
  `window.location.origin`. In the Tauri webview that origin is `tauri://…`, not the
  web app. The desktop builds the URL from `webAppBaseUrl` (from
  `getDesktopConfig()`):
  - `public`  → `${webAppBaseUrl}/shared/meeting-notes/${shortCode}`
  - otherwise → `${webAppBaseUrl}/meeting-notes/${shortCode}`
  Newly-shared meetings can use the `shareUrl` returned by `shareMeeting`.
- **Unshare confirmation.** The desktop has no `ConfirmDialog`/`Modal` component.
  Instead of a modal overlay, clicking a shared avatar's ✕ opens a **lightweight
  inline confirm bar** at the top of the popover body: "Unshare with {email}?
  [Cancel] [Unshare]". Single target at a time; Cancel/blur dismisses.
- **Clipboard.** No existing clipboard usage in the app. Use
  `navigator.clipboard.writeText` (supported in the Tauri v2 webview) with a
  try/catch fallback that alerts the URL — mirrors the reference's `copyToClipboard`.

## Data flow

**Ariso:**

1. User opens a meeting → `MeetingDetailView.load` → `Backend.getMeetingDetail` →
   `getMeetingNotes` (now also returns share-gating fields) → `detail`.
2. User clicks **Share** → `showShare = true` → popover mounts → fetches
   `webAppBaseUrl` + `listShareEmails`.
3. **Email invite:** `sendShareEmail` → on success add to local set / show
   already-shared; on error show inline message.
4. **Unshare:** avatar ✕ → inline confirm → `unshareEmail` → remove from local set.
5. **Visibility / public:** `selectSharingOption` → `shareMeeting` (public requires
   the expiry Save) → update `detail` + local `shareUrl`.
6. **Copy link:** builds URL from `webAppBaseUrl` + `shortCode` + `visibility`,
   writes to clipboard, shows "Copied!".

**Local:**

1. User opens a local recording → `detail.note` loaded (AI note); personal note
   is loaded lazily.
2. User clicks **Share** → capture button `getBoundingClientRect()` →
   `notesPersistence.load(item)` for the personal note →
   `composeLocalShareText(detail, personalNote)` → `shareTextNative(text, rect)` →
   native macOS share sheet opens anchored to the button.

## Error handling

- All HTTP failures surface the server's `data.error` where present, else a generic
  message, shown inline in the popover (no global toast system exists).
- `listShareEmails` failures collapse to `[]` (passive, non-critical).
- Stale-response guard: the popover is keyed to a single `meetingId`; the parent
  unmounts/remounts it on meeting change, so no in-flight cross-meeting races.

## Testing

- **`useMeetingApi` methods** (vitest, mocking `api.request`): `shareMeeting` body +
  return mapping, `listShareEmails` parsing + error→[], `sendShareEmail`
  already-shared + error message, `unshareEmail` URL encoding.
- **`ShareMeetingPopover.vue`**: permission gating (`canSharePublic` for each
  `shareMeetingNotesToPublic` value × role), copy-link URL construction for
  public vs workspace, email send/unshare flows incl. inline confirm, expiry
  validation (1–365), `isHost`/`isAttendee` derivation.
- **`composeLocalShareText`**: both sections present; AI-only; personal-only;
  neither (→ `''`); heading formatting.
- **`MeetingDetailView`**: Share button visibility (shown for local; for Ariso only
  host/attendee), branch routing (local → `shareTextNative` with composed text;
  Ariso → popover toggle), removal of the stub copy-link button.
- **Rust `share_text_native`**: objc2 UI can't be unit-tested meaningfully; verify
  manually (sheet opens, anchors to the button, Copy/AirDrop/Mail populated). The
  Y-flip coordinate conversion can be a small pure helper with a unit test.

## Risks / open questions

- `navigator.clipboard` permission in the packaged Tauri build — fallback alert
  covers denial; verify during implementation.
- Confirm the live server response field names (`already_shared`,
  `publicShareExpiresAt`, `items[].email`, `shareMeetingNotesToPublic`) match the
  reference's usage — they are the same endpoints the web app consumes today.
- `NSSharingServicePicker` must run on the main thread and needs the focused
  window's content `NSView`; confirm the Tauri window handle → `NSView` bridge
  (via `objc2-app-kit` / the window's `ns_window()`) during implementation.
- `objc2-app-kit` version must align with the existing `objc2` 0.6 / `objc2-
  foundation` 0.3 in `Cargo.toml`.
</content>
</invoke>
