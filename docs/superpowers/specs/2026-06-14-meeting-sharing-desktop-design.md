# Meeting Sharing in the Desktop Detail Panel — Design

**Date:** 2026-06-14
**Status:** Approved (pending spec review)

## Goal

Make the **Share** button in `MeetingDetailView.vue`'s header fully functional for
the **Ariso backend**, porting the meeting-sharing feature from the web app's
`apps/web-ui/src/Pages/MeetingNotesDetail.vue` (reference) into the desktop app
(`oats`). Full feature parity with the reference's share dropdown, adapted to the
desktop's environment and visual language.

Local recordings (`LocalBackend`) have no sharing concept; the Share button is
hidden for them.

## Scope

**In scope (full parity with the reference share dropdown):**

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

**Out of scope (not requested):** Google-Docs / markdown export, delete-meeting,
attendees-edit modal, coaching drawer, transcript export.

## Decisions (confirmed with user)

1. **Full parity** for the share feature.
2. **Remove** the existing standalone header copy-link icon button; copy-link lives
   inside the share dropdown (matching the reference).
3. **Match the web's permission gating exactly.**

## Architecture (Approach B: dedicated popover component)

The codebase already has two clean seams:

- `Backend` (`useBackend.ts`) — abstracts **local-vs-ariso storage** (list, detail,
  transcript, individual note, rename).
- `useMeetingApi.ts` — the **HTTP layer** (`api.request(method, path, body)` →
  `{ status, data }`; no axios interceptors, no `params`/`suppressGlobalError`).

Sharing is an Ariso-only, server-side feature with **no local-recording
equivalent**, so its write/HTTP calls live in `useMeetingApi` and are called
directly by the popover (not threaded through the `Backend` interface, which would
force `LocalBackend` to stub Ariso-only methods). The *read* path for gating fields
still flows through `Backend.getMeetingDetail` so the detail panel has one load
path.

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

4. **`MeetingDetailView.vue` — wiring:**
   - Remove the stub Share button's no-op and the standalone copy-link icon button.
   - Render a Share button only when `!detail.isLocal && (isHost || isAttendee)`
     (host/attendee derived as above). Toggles `showShare`.
   - Render `<ShareMeetingPopover :detail="detail" :meeting-id="detail.id"
     @close="showShare = false" />` anchored under the header when `showShare`.
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
- **`MeetingDetailView`**: Share button visibility (hidden for local / non-
  host-attendee), popover toggle, removal of the stub copy-link button.

## Risks / open questions

- `navigator.clipboard` permission in the packaged Tauri build — fallback alert
  covers denial; verify during implementation.
- Confirm the live server response field names (`already_shared`,
  `publicShareExpiresAt`, `items[].email`, `shareMeetingNotesToPublic`) match the
  reference's usage — they are the same endpoints the web app consumes today.
</content>
</invoke>
