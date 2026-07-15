# View prep in the meeting detail view — design

Date: 2026-07-15
Status: approved

## Summary

When a meeting has an associated meeting prep, the desktop's meeting detail view
shows a slim "Meeting prep is ready" banner with a **View prep** button. Clicking
it fetches the prep from the Ariso API and renders its markdown content in the
detail card's content area, replacing the active tab pane until dismissed.

Today the only desktop surface for meeting prep is the native OS notification
(`meeting_notifications.rs`), which deep-links to the **web app**
(`/my/meeting-prep-v2/{prepId}`). This feature makes the prep readable in-app.

## API contract (verified against production responses)

- `GET /meetings?start_date=…&end_date=…` (and the `?q=` search variant): each
  meeting row includes `prep_id: number` **when a prep exists**; the field is
  absent otherwise.
- `GET /meeting-preps/{prepId}` returns:

  ```json
  {
    "meetingPrep": {
      "id": 4339,
      "meeting_id": "37480",
      "content": "## Open items…  (markdown)",
      "attendees": [...],
      "previous_meetings": [...],
      "meeting_url": "…",
      "…": "…"
    }
  }
  ```

Only `content` is consumed. `attendees`, `previous_meetings`, `meeting_url`,
and the rest are deliberately ignored (YAGNI).

## Data flow

Ariso backend only. Local recordings never have preps, so nothing on the local
path changes and the banner can never render for a local meeting.

1. **`useMeetingApi.ts`**
   - `ScheduledMeeting` gains `prep_id?: number | string` (tolerant of the
     API returning either, mirroring `auto_join_scheduled`'s leniency).
   - New `getMeetingPrep(prepId: number): Promise<string | null>` —
     GET `/meeting-preps/{prepId}`; resolves the `meetingPrep.content` string,
     `null` on 404 or when `content` is missing/empty (matching the
     `getMeetingTranscript` "absent ≠ error" convention); throws on other
     non-200s via `assertOk`.
2. **`useBackend.ts`**
   - `MeetingListItem` gains `prepId?: number`; `meetingSummaryToListItem`
     sets it from `prep_id` when it parses to a finite number (list and
     search rows share this mapper, so search hits get it for free).
   - `MeetingDetail` gains `prepId?: number`; `ArisoBackend.getMeetingDetail`
     carries it from the list item, exactly how `autoJoinScheduled` travels
     today — `/meeting-notes/:id` does not include it.
3. **`MeetingDetailView.vue`** owns fetch-on-demand, caching, and display
   state (below).

## UI

### Banner

- Rendered between the meta band and the tab bar, only when
  `detail.prepId != null`.
- Content: "✨ Meeting prep is ready" left, a **View prep** button right.
- While the prep pane is open the button reads **Hide prep**.
- No prep id → no banner, zero layout change for every other meeting.

```
┌──────────────────────────────────────────────┐
│ Max + Shawn Weekly            [Share] [✕]    │
│ ⏱ 40m · 👤 2 Attendees · #one-on-one         │
│ ✨ Meeting prep is ready      [View prep]    │
│──────────────────────────────────────────────│
│ [AI Notes][Transcript][My note]              │
│──────────────────────────────────────────────│
│ Quick Digest…                                │
└──────────────────────────────────────────────┘
```

### Prep pane

- Clicking **View prep** switches the content area to a prep pane; no tab
  segment renders active while it is open.
- The pane lazily fetches `getMeetingPrep(detail.prepId)` on first open and
  caches the result for the lifetime of the loaded meeting. States:
  - loading: spinner + "Loading prep…" (same `card-state` pattern as
    transcript loading);
  - loaded: `content` rendered through the existing `renderMarkdown` into a
    `.md` block;
  - `null` (404 / empty): "No prep available." empty state;
  - error: inline error text in the pane.
- Exiting: clicking **Hide prep** returns to the previously active tab;
  clicking any tab segment also closes the pane and activates that tab.
- Meeting switch resets prep state (open flag, cache, in-flight guard) using
  the view's existing `reqId` pattern so a stale response can't land on the
  wrong meeting.

Links inside the prep markdown (e.g. "Previous meeting note") behave exactly
like links in AI Notes today — no new URL-opening surface is introduced.

## Testing (Vitest)

- `useMeetingApi.test.ts` — `getMeetingPrep`: happy path resolves `content`;
  404 → `null`; missing/empty `content` → `null`; other non-200 throws.
- `useBackend.test.ts` — `prep_id` mapping: numeric and numeric-string map to
  `prepId`; absent/garbage stays `undefined`; `getMeetingDetail` carries
  `item.prepId` through.
- `MeetingDetailView.test.ts` — banner absent without `prepId`; present with
  it; View prep click fetches and renders markdown; Hide prep / tab click
  restores the tab; fetch error shows the error state. (Heavy view test file:
  run in isolation per repo convention.)

## Out of scope

- Rendering prep attendees / previous meetings / meeting URL.
- Prep on the UpNextCard or library rows.
- Any change to the notification deep link (still opens the web app).
- Offline/local backend behavior.
