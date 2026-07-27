# View prep in the meeting detail view — design

Date: 2026-07-15 (revised 2026-07-24)
Status: approved

## Summary

When a meeting has an associated meeting prep, the desktop's meeting detail view
shows a **Prep** tab next to **My Notes**. Selecting it fetches the prep from the
Ariso API and renders its markdown in the detail card's content area.

The "Meeting prep ready" OS notification now opens that tab *in-app* instead of
deep-linking to the web app: clicking it opens (or focuses) the Library window,
selects the prep's meeting, and activates its Prep tab.

## API contract (verified against production responses)

- `GET /meetings?start_date=…&end_date=…` (and the `?q=` search variant): each
  meeting row includes `prep_id: number` **when a prep exists**; the field is
  absent otherwise.
- `GET /meeting-preps/{prepId}` returns:

  ```json
  {
    "meetingPrep": {
      "id": 5145,
      "meeting_id": "45565",
      "content": "## Standup prep…  (markdown)",
      "attendees": [...],
      "previous_meetings": [...],
      "meeting_url": "…",
      "…": "…"
    }
  }
  ```

Only `content` and `meeting_id` are consumed — `content` to render, `meeting_id`
to resolve a notification back to its meeting. `attendees`,
`previous_meetings`, `meeting_url`, and the rest are deliberately ignored (YAGNI).

## Realtime trigger

The prep-ready signal is the generic inbox event, not a prep-specific one:

```
channel: private-{orgId}-{orgUserMappingId}
event:   inbox-message-ready
data:    { source: "meeting_prep", sourceId: <meetingPrepId> }
```

`meeting_notifications.rs` acts only on `source === "meeting_prep"` and ignores
every other inbox source. `sourceId` is tolerated as a number or numeric string.
The catch-up backstop (`/user-inbox-messages`, run on every re-subscribe) is
unchanged — it already filters on the same `meeting_prep` source.

## Data flow

Ariso backend only. Local recordings never have preps, so nothing on the local
path changes and the Prep tab can never render for a local meeting.

1. **`useMeetingApi.ts`**
   - `ScheduledMeeting` gains `prep_id?: number | string` (tolerant of the
     API returning either, mirroring `auto_join_scheduled`'s leniency).
   - `getMeetingPrep(prepId: number): Promise<MeetingPrep | null>` —
     GET `/meeting-preps/{prepId}`; resolves `{ content, meetingId }`, `null` on
     404 or when the payload carries no `meetingPrep` (matching the
     `getMeetingTranscript` "absent ≠ error" convention); throws on other
     non-200s via `assertOk`. `content` is null on its own when the markdown is
     missing/blank; `meetingId` is null when `meeting_id` is missing/unusable.
2. **`useBackend.ts`**
   - `MeetingListItem` gains `prepId?: number`; `meetingSummaryToListItem`
     sets it from `prep_id` when it parses to a finite number (list and
     search rows share this mapper, so search hits get it for free).
   - `MeetingDetail` gains `prepId?: number`; `ArisoBackend.getMeetingDetail`
     carries it from the list item, exactly how `autoJoinScheduled` travels
     today — `/meeting-notes/:id` does not include it.
3. **`MeetingDetailView.vue`** owns fetch-on-demand, caching, and tab state.
4. **`LibraryView.vue`** owns the notification-click landing (below).

## UI

### Prep tab

- Appended after **My Notes** (before **AI Assessment**) whenever
  `detail.prepId != null`. No prep id → no tab, zero layout change for every
  other meeting.

```
┌──────────────────────────────────────────────┐
│ Max + Shawn Weekly            [Share] [✕]    │
│ ⏱ 40m · 👤 2 Attendees · #one-on-one         │
│──────────────────────────────────────────────│
│ [AI Notes][Transcript][My Notes][Prep]       │
│──────────────────────────────────────────────│
│ ## Standup prep …                            │
└──────────────────────────────────────────────┘
```

- The pane lazily fetches `getMeetingPrep(detail.prepId)` on first open and
  caches the result for the lifetime of the loaded meeting. States:
  - loading: spinner + "Loading prep…" (same `card-state` pattern as
    transcript loading);
  - loaded: `content` rendered through the existing `renderMarkdown` into a
    `.md` block;
  - no content (404 / empty): "No prep available." empty state;
  - error: inline error text in the pane.
- Meeting switch resets prep state (cache, in-flight guard) using the view's
  existing `reqId` pattern so a stale response can't land on the wrong meeting.
- The tab is only the *default* tab when nothing else has content
  (`firstTabFor`), so opening a meeting normally still lands on AI Notes.
- `openPrepTab(meetingId)` is exposed for the notification path. The request
  races the detail load the Library's selection just started, so it is **keyed
  by meeting id**, not a bare flag: it is answered from `detail` only when that
  detail is the wanted meeting's (mid-switch it still holds the *previous*
  meeting's, Prep tab and all), otherwise it is held as a pending id that
  `load()` applies once the tabs exist — and `load()`'s state reset keeps a
  pending id that names the meeting it is loading. Either landing order works.
- The detail reloads when `item.prepId` changes (it is carried from the row), so
  a row that gains a prep id after the list loaded grows its Prep tab.

Links inside the prep markdown (e.g. "Previous meeting note") behave exactly
like links in AI Notes today — no new URL-opening surface is introduced.

### Notification click → in-app

The macOS `UNUserNotificationCenter` request identifier carries `oats-prep://{id}`
(an internal token, not a registered URL scheme). On click the delegate — already
on the main thread, which window creation requires:

1. stores the prep id in a native pending slot,
2. opens (or focuses) the Library window,
3. emits `meeting-prep://open`.

The id travels through the slot rather than the event payload because the click
usually *creates* the Library window, so an emitted payload would arrive before
any listener exists. `LibraryView` claims it with the
`take_pending_meeting_prep` command both on mount and on `meeting-prep://open`
(the already-open case); the claim clears the slot, so a prep opens exactly once.

Resolving the prep to a row:

1. match a loaded row on `prepId`;
2. if none, `loadMeetings()` and retry — the list can predate the prep;
3. if still none, fall back to `getMeetingPrep(prepId).meetingId`: match a
   loaded row by id (patching the known `prepId` onto it, since a row that
   predates the prep carries none and would render no Prep tab), otherwise fetch
   that meeting by id and **pin** it the way an ad-hoc recorded meeting is
   pinned — this is what covers a meeting outside the loaded window;
4. if still none (no `meeting_id`, or the meeting can't be fetched), log and
   leave the pane untouched.

Then select the row — replacing the selected item when the same row was already
selected, so a patched `prepId` reaches the pane — and call the detail view's
`openPrepTab(meetingId)`.

In dev / unsigned builds UNC is unavailable and the notification falls back to
the Tauri plugin, which exposes no click handling — the in-app landing only
works in a Developer-ID-signed bundle. Unchanged from the previous behavior.

## Testing

- Rust (`meeting_notifications.rs`) — `inbox-message-ready` selector: numeric
  and numeric-string `sourceId`; non-`meeting_prep` sources ignored; missing
  `sourceId` ignored; Pusher's JSON-encoded `data` decodes; `oats-prep://` link
  round-trips and rejects foreign identifiers.
- `useMeetingApi.test.ts` — `getMeetingPrep`: content + meetingId happy path;
  numeric `meeting_id`; 404 → `null`; no `meetingPrep` → `null`; missing/empty
  `content` → null content; missing `meeting_id` → null meetingId; other
  non-200 throws.
- `useBackend.test.ts` — `prep_id` mapping: numeric and numeric-string map to
  `prepId`; absent/garbage stays `undefined`; `getMeetingDetail` carries
  `item.prepId` through; `getMeetingPrep` delegates.
- `MeetingDetailView.test.ts` — no Prep tab without `prepId`; tab order places
  Prep after My Notes; the tab fetches once and renders markdown; switching tabs
  leaves the pane; `openPrepTab` activates it; null / empty / error states.
  Race coverage: a request that lands while the meeting is still loading, one
  that lands while a pending note save delays that load's reset, and a stale
  request naming the previously selected meeting (ignored).
- `LibraryView.test.ts` — claims a queued prep on mount and opens the meeting's
  Prep tab (passing the meeting id); the `meeting_id` fallback; the prep id
  patched onto a row that predates the prep; pinning a meeting outside the
  loaded list; the `meeting-prep://open` event path; no-op when nothing is
  queued or the prep's meeting can't be reached; the row is highlighted in the
  sidebar; plus one end-to-end pass with the real detail view mounted.

(Heavy view test files: run in isolation per repo convention.)

## Out of scope

- Rendering prep attendees / previous meetings / meeting URL.
- Prep on the UpNextCard or library rows.
- Offline/local backend behavior.
