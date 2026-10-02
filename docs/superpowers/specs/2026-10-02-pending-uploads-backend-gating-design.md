# Hide the pending-uploads box on the local backend (issue #464)

## Problem

`src/views/PendingUploads.vue` renders its "Pending uploads" box purely based
on `items.length > 0` (`src/views/PendingUploads.vue:2`) — it has no idea
which backend is active. It's mounted unconditionally from
`LibraryView.vue:239`, inside the sidebar `<aside>`.

Pending-upload buffers (`~/.ariso/pending-uploads`, written by
`ArisoBackend.finalizeRecording` in `src/composables/useBackend.ts`) only
exist because the Ariso cloud backend buffers audio before/during upload as a
crash-safety net. The local backend never uploads anything, so if a stale
buffer is ever present while local is active (e.g. left over from a prior
Ariso session, or the user switched away before a retry finished), the box
surfaces a cloud-upload queue that doesn't apply to the user's current
configuration — confusing noise, per the issue.

## Goal

- The pending-uploads box is visible only while the Ariso backend is active.
  Switching to local hides it immediately (reactively, no remount of the
  whole Library needed); switching back to Ariso shows it again (with a
  fresh read of the on-disk buffer list).
- No change to what counts as a "pending upload" or how buffers are written,
  combined, uploaded, or discarded — this is a display-layer fix only.

## Non-goals (per the issue's "only build what's described")

- Changing `finalizeRecording`/`useBackend.ts` so buffers are never written
  while on the local backend. Today only `ArisoBackend.finalizeRecording`
  calls `pending.bufferAudio`; `LocalBackend` never does. So this scenario is
  already naturally rare (it would require a leftover buffer from a *prior*
  Ariso session), and the issue's acceptance criterion is about display, not
  buffer lifecycle.
- Discarding or otherwise cleaning up stale buffers when switching to local.
  They stay on disk, unseen, until the user switches back to Ariso — at which
  point they reappear and remain uploadable/discardable exactly as before.
- Any change to `PendingUploads.vue` internals, `usePendingUploads.ts`, or
  the Rust `storage.rs` pending-upload commands.
- Touching `src-tauri/src/capabilities/`, `tauri.conf.json`, or any other
  forbidden path — none of this needs them.

## Design

`LibraryView.vue` already tracks the active backend reactively in
`activeBackend` (a `ref<Backend | null>`, `src/views/LibraryView.vue:453`),
kept current by `loadMeetings()` on every backend switch (`activeBackend.value
= backend`, set from `getActiveBackend()`) and by the existing
`BACKEND_CHANGED_EVENT` listener → `onBackendChanged()` → `loadMeetings()`
chain. This is the same ref an existing sibling element already gates on
(`v-if="activeBackend?.supportsSearch"` on the search trigger button,
`LibraryView.vue:225`).

Gate the `<PendingUploads>` element itself the same way, by backend id rather
than a new `Backend` capability flag — unlike `supportsSearch`/
`supportsActionItems`, "has a pending-uploads box" isn't a capability any
other call site needs to query, so a new interface field would be unused
abstraction for a single `v-if`:

```vue
<!-- Only Ariso buffers to a cloud-upload queue; the local backend never
     uploads, so the box would just be noise there. -->
<PendingUploads
  v-if="activeBackend?.id === 'ariso'"
  ref="pendingUploads"
  @uploaded="onPendingUploaded"
/>
```

Consequences, all already-acceptable existing patterns in this component:

- Before `activeBackend` resolves on first mount, the box is hidden (same
  brief hidden-until-resolved window the search trigger already has).
- Switching backends unmounts/remounts the component (Vue's normal `v-if`
  behavior). Remounting already performs a fresh `pending.list()` read via
  its `onMounted` hook — the exact behavior the codebase already relies on
  elsewhere (see the comment at `LibraryView.vue:1316`,
  "remounting PendingUploads also performs a fresh initial read").
- The `pendingUploads` template ref becomes `null` while unmounted. Every
  existing call site already uses optional chaining
  (`pendingUploads.value?.refresh()`), so no call site needs a guard added.

No change to `PendingUploads.vue`, `usePendingUploads.ts`, or any Rust code.

## Decisions

Non-interactive run (`autofix:approved`); the trusted maintainer comment
settles the top-level requirement, everything else defaults per the
autopilot contract.

1. **What is the acceptance criterion?**
   Trusted comment (`shawnzhu`): "do not show the pending upload box when
   switching to Local backend." Carried forward verbatim into Acceptance
   criteria below.
2. **Should this react live to backend switching within the same session (via
   `BACKEND_CHANGED_EVENT`), or is remount-on-switch sufficient?**
   Default (used): live — `activeBackend` already updates reactively on
   every switch (same-window via `switchBackend`, or another window via the
   `BACKEND_CHANGED_EVENT` listener), so a plain `v-if` against it reacts
   immediately with no extra plumbing.
3. **Should stale buffers on disk be hidden-but-later-uploadable after
   switching back to local, or should switching to local trigger
   cleanup/discard of them?**
   Default (used): hidden but preserved — switching backends must never
   discard user data as a side effect of a display fix; the box simply
   reappears, fully functional, on switching back to Ariso.
4. **Does the underlying pending-upload creation path
   (`finalizeRecording`/`useBackend.ts`) also need gating so buffers never
   get written while on local backend?**
   Default (used): no — out of scope. `LocalBackend.finalizeRecording`
   already never calls `pending.bufferAudio`; only `ArisoBackend` does. This
   is a display-layer fix per the issue's acceptance criterion.
5. **Gate via a new `Backend.supportsPendingUploads` capability flag (matching
   the `supportsSearch`/`supportsActionItems` pattern) or a direct `id ===
   'ariso'` check at the one call site?**
   Default (used): direct id check — no other call site needs this as a
   capability query, so a new interface field (and the mock-object updates it
   would force across every test that constructs a `Backend`) is unused
   abstraction for a single conditional.

## Testing

- `src/views/LibraryView.test.ts`:
  - Updated the two existing tests that mounted `LibraryView` with a stubbed
    `PendingUploads` and asserted it renders, and the "notifies the recorder
    window when a sidebar pending upload succeeds" test — all three now
    explicitly set the backend to `'ariso'` (the default test backend is
    `'local'`, which would now hide the stub).
  - Added: renders on Ariso, hidden on local (default backend), and hidden
    immediately after a same-session switch from Ariso to local via the
    `BACKEND_CHANGED_EVENT` listener (`backend://changed` with a different
    `source`, matching how another window's switch is picked up).
- `src/views/PendingUploads.test.ts`: unchanged — the component itself has no
  backend awareness, so none of its existing behavior changes.
- Ran `npm test` (full suite), `npm run vite:build`, and
  `cargo build --locked --manifest-path src-tauri/Cargo.toml` — all green.
- Not verifiable in this environment: manually switching backends in the
  running app and observing the box appear/disappear. Covered instead by the
  `LibraryView.test.ts` tests exercising the same reactive path
  (`activeBackend` + the `BACKEND_CHANGED_EVENT` listener) the real UI uses.

## Acceptance criteria

- [x] The pending-upload box is not shown when the active backend is local.
- [x] The pending-upload box is shown when the active backend is Ariso (no
      regression to existing behavior).
- [x] Switching backends within the same session updates visibility without
      requiring a full window reload.
- [x] No change to pending-upload buffering, combine, upload, or discard
      behavior.
