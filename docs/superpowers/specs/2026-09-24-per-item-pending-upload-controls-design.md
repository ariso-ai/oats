# Per-item pending upload controls (issue #451)

## Problem

`PendingUploads.vue` (the panel shown in the Library sidebar when Ariso uploads are
buffered locally) already lets a user act on a single buffered recording for
**discard** — each row has a per-recording `Discard`/`Confirm` text button, wired to
`onDiscardItem` → `pending.discardAudio`. But there is no per-recording **upload**:
the only way to push one buffered recording to Ariso is the bulk `Upload (N)` button,
which combines and uploads every pending item via `combineAndUpload`
(`src/composables/usePendingUploads.ts`). A user who wants to keep some recordings
buffered and only send one up has no way to do that without uploading everything.

Separately, the per-recording Discard button today renders as text (`Discard` /
`Confirm`) and is hidden entirely when there's exactly one pending item (on the
reasoning that "Discard all" already discards exactly one item in that case). The
issue's acceptance criteria calls for icon buttons, for every recording.

## Goal

Every row in the pending-uploads list gets two icon-only controls: **Upload this
recording** and **Delete this recording**, in addition to the existing Play and
Locate controls. Each acts on exactly that one buffered recording, independent of
the others. The existing bulk `Upload (N)` / `Discard all` actions are unchanged.
The UI updates immediately and accurately reflects each item's state (in-flight,
succeeded/removed, failed-with-error) after either action.

## Non-goals

- Changing the bulk upload/discard actions' behavior, labels, or the
  `combineAndUpload`/`discardAll` functions they call.
- Changing `PartialUploadError` or partial-failure handling for the *bulk* upload
  path — untouched.
- Multi-select (checkbox) bulk actions on a subset of items — out of scope; this is
  strictly one-button-per-row.
- Any change to the Rust/Tauri commands (`combine_pending_audio`,
  `discard_pending_audio`, upload presign endpoints) — the frontend already has
  everything it needs (`pending.combine`, `pending.discardAudio`,
  `useMeetingApi().uploadAudio`).
- Converting the Locate button, or the bulk Upload/Discard buttons, to icons —
  the acceptance criteria only calls out the per-recording upload/delete controls.

## Decisions

Clarify questions, decided non-interactively per the autopilot run (no human present,
no trusted comment on this issue — every answer below is the recon-derived default):

1. **Should the per-recording Delete control always render, even when only one item
   is pending (overriding the existing `v-if="items.length > 1"` on `.pi-discard`)?**
   Default (chosen): yes, always render both per-recording controls regardless of
   count. The acceptance criteria says "for each recording" without a count
   exception, and icon buttons cost far less row space than the old text buttons did
   — the original "Discard all already covers the single-item case" argument doesn't
   carry over now that identical per-item and bulk actions aren't visually redundant
   text. This does mean a single-item row now shows both a per-item and a bulk
   control that do the same thing; that's an acceptable, minor redundancy given the
   acceptance criteria's explicit wording.
2. **Does per-recording Upload act on just that one buffered item, or on its whole
   `meetingId` group (relevant for a resumed/split recording whose segments share a
   `meetingId`)?** Default (chosen): exactly that one item's buffer — same
   granularity the existing per-recording Discard already uses (`pending.discardAudio
   (it.createdAt)` discards one buffer regardless of grouping). Mirroring that
   granularity is the one behavior a user can already observe and rely on; scoping
   Upload to the whole group would make Upload and Delete inconsistent with each
   other for the exact same multi-segment case.
3. **Do the bulk `Upload (N)` / `Discard all` buttons also become icons?** Default
   (chosen): no, they keep their current text+count style. The acceptance criteria's
   icon requirement is scoped to "each recording" (i.e. the new per-row controls);
   the bulk buttons' count and "all" wording carry information an icon alone would
   drop.
4. **Does per-recording Upload check `auth.checkSession()` first, like the bulk
   Upload does?** Default (chosen): yes — same pre-flight check, same
   sign-in-required message via the existing `uploadErrorMessage` helper, so a
   signed-out user gets the same specific message from either control instead of a
   generic failure from one of them.
5. **Icon source, given no icon library is installed?** Default (chosen): inline SVG
   using this repo's existing convention (`viewBox="0 0 24 24" class="ic"`, seen in
   `MeetingDetailView.vue`). Reuse that file's trash-can path for Delete; for Upload,
   mirror its download-tray path with the arrow direction flipped (tray + up-arrow)
   rather than pulling in a new dependency.

## Interfaces

### `src/composables/usePendingUploads.ts`

Add one exported function alongside `combineAndUpload`/`discardAll`:

```ts
/** Upload exactly one buffered item (the per-recording "Upload" action) — not its
 *  whole meetingId group, matching per-recording Discard's granularity. Resolves to
 *  the meeting the audio landed on so the caller can mark it "processing". */
export async function uploadItem(item: PendingUploadMeta): Promise<number>
```

Implemented by extracting the existing private `uploadGroup` body's error-reporting
(`attempt: 'retry'`, stage-tagged) and calling it with a single-item array — no
change to `uploadGroup`'s signature or to `combineAndUpload`.

### `src/views/PendingUploads.vue`

- New per-row state:
  - `uploadingItem = ref<string | null>(null)` — the `createdAt` of the item
    currently mid-per-item-upload (mirrors `discardingItem`, kept separate from
    `busy` so it doesn't drive the bulk button's spinner).
  - `actionsDisabled` extended to also cover `uploadingItem.value !== null`, so a
    per-item upload locks every other action the same way a bulk upload or a
    per-item discard already do (discarding a buffer mid-upload could pull it out
    from under the request).
- New handler `onUploadItem(it: PendingUploadMeta)`:
  - Disarms any armed discard confirmation (bulk or per-item), clears `error`.
  - Checks `auth.checkSession()`; on failure sets the same sign-in message
    `uploadErrorMessage` already produces and returns without calling `uploadItem`.
  - Sets `uploadingItem.value = it.createdAt`, calls `uploadItem(it)`.
  - On success: `processing.markUploaded(meetingId)`, `await refresh()`,
    `emit('uploaded')` — matching the bulk path's post-upload bookkeeping.
  - On failure: `error.value = uploadErrorMessage(e)`, item stays in the list (no
    `PartialUploadError` case here — it's a single group, so any rejection is a
    full failure of that one item).
  - `finally`: `uploadingItem.value = null`.
- Template: two new icon-only buttons per row inside `.pi-controls`, alongside the
  existing Locate/Discard, both `v-if`-unconditional (always rendered — see Decision
  1):
  - `.pi-upload` — upload-tray SVG; `disabled="actionsDisabled"`; shows a small
    spinner (reuse the existing `.spinner` style) in place of the icon while
    `uploadingItem.value === it.createdAt`; `aria-label`/`title` "Upload this
    recording now".
  - `.pi-discard` — becomes icon-only (trash-can SVG) instead of `Discard`/`Confirm`
    text; drop its `v-if="items.length > 1"` guard; keep the existing `armed` class
    and two-click confirm behavior unchanged; `aria-label`/`title` switches between
    "Delete this buffered recording without uploading it" (unarmed) and "Click again
    to permanently delete this recording" (armed) so the state change is
    announced to assistive tech, not just conveyed by color.
  - Icon buttons get `type="button"` and an explicit `aria-label` (not just `title`)
    since they carry no visible text.

## Acceptance criteria

(Carried from the issue body and its acceptance-criteria section; none dropped or
weakened.)

- [ ] Each pending item in the pending-uploads view has its own action to upload
      that item alone.
- [ ] Each pending item has its own action to delete/discard that item alone.
- [ ] Both per-item actions use an icon instead of text.
- [ ] The existing bulk "Upload all" and "Discard all" actions remain available and
      unchanged.
- [ ] The UI updates immediately and accurately reflects each item's state after a
      per-item action, including failures (item removed on success; error message
      and item retained on failure; other rows stay interactive/unaffected by one
      row's failure).
- [ ] Per-item actions are accessible: icon-only buttons have `aria-label`/`title`
      text describing the action and its state (e.g. armed-to-confirm).

## Test strategy

Extend the existing Vitest suites (both already mock `pending`/`auth`/`useMeetingApi`
so no new mocking infra is needed):

`src/composables/usePendingUploads.test.ts`:
- `uploadItem` combines the single key, uploads with that item's own (unmerged)
  meta, discards that one buffer, and returns the meeting id.
- `uploadItem` reports a `combine`-stage vs an upload-stage failure the same way
  `combineAndUpload` does, with `itemCount: 1`.
- `uploadItem` does not discard the buffer when the upload rejects.

`src/views/PendingUploads.test.ts`:
- Per-recording Upload button exists for every row, including when only one item is
  pending (replaces/extends the existing "shows no per-recording Discard when only
  one recording is pending" test, which inverts to "still shows" per Decision 1).
- Clicking a row's Upload calls `uploadItem` with that row's item only, marks its
  returned meeting id as processing, refreshes, and emits `uploaded`.
- A failed per-recording upload shows the error message, keeps that item (and any
  others) in the list, and does not call `processing.markUploaded`.
- Per-recording Upload checks the session first and shows the sign-in message
  without calling `uploadItem` when signed out (mirrors the existing bulk-upload
  session test).
- Per-recording Upload and Discard buttons are `disabled` while any upload/discard
  (bulk or per-item) is in flight (extends the existing "disables per-recording
  Discard while an upload is in flight" test to also cover the new Upload button,
  and to cover per-item-upload-in-flight disabling the *other* row's buttons).
- Per-recording Discard is icon-only now (assert on `aria-label`/`title` text and
  the `armed` class, not on button text content — updates the existing text-based
  assertions in the "confirms then discards", "does not let another recording's
  click discard", "cancels ... when the pointer leaves its row", and "shows an
  error and keeps the recording" tests).

No new Rust tests — no backend interface changes.

## Risk

- `PendingUploads.test.ts` has several existing assertions on `.pi-discard`'s text
  content (`'Discard'`, `'Confirm'`) that must be updated to the icon-based
  attribute checks described above, or they'll fail against the new markup — this
  is expected churn, not a regression, but the implementer should update rather than
  delete coverage.
- `actionsDisabled` growing a third source of truth (`uploadingItem`) needs care:
  a per-item upload must not visually re-enable a per-item discard on an *other* row
  (or vice versa) while it's running — the existing `discardingItem`/`busy` pair
  already sets this precedent (both lock every row, not just their own), so
  `uploadingItem` should follow the same all-rows-lock pattern for consistency, even
  though only one row is actually busy.
