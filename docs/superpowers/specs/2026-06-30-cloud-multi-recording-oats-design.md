# Cloud (Ariso) multi-recording support in oats — design

**Date:** 2026-06-30
**Status:** Approved (brainstorming) → ready for implementation plan
**Scope:** PR 1 of 2. This spec covers the **Ariso (cloud)** backend only. The
**Local (offline)** backend is a separate follow-up PR with its own spec (see
§8).

## 1. Problem

A single meeting can now be recorded more than once — e.g. you record a
calendar meeting, stop (break, accidental stop, app quit), then record more of
the *same* meeting shortly after. Each recording session should become an
additional **clip** on that meeting rather than overwriting the previous one or
creating a brand-new meeting.

The **cloud backend already supports this**. The agents-repo series
(PRs 5918–5921, merged; 5922 incremental summary open) made each audio upload a
first-class clip: a per-upload `transcript_id`, an S3 key
`{org}/meetings/{id}/audio/{transcriptId}.{ext}`, a `meeting_audio_files` row,
per-clip transcription, and a read surface exposing `audio_clips` plus per-clip
audio/transcript. web-ui (PR 5921/5920) already renders stacked players,
per-clip transcript switching, and host-only per-clip delete.

This PR brings oats to parity on the desktop side.

## 2. Current state in oats (what already works)

**Resume trigger = same calendar meeting.** An auto- or manually-triggered
recording resolves its target meeting via
`resolveAssociation` → `pickDefaultMeeting`, which returns the calendar
meeting's own `id` (`src/composables/useAutoTrigger.ts:16`,
`src/composables/pickDefaultMeeting.ts`). Recording the same calendar meeting
again resolves the **same `meetingId`**.

**Producing path re-attaches cleanly.** `ArisoBackend.finalizeRecording`
(`src/composables/useBackend.ts:256`) buffers then calls `uploadAudio`
(`src/composables/useMeetingApi.ts:457`) →
`POST /desktop/meetings/{id}/audio/presign` → PUT → `…/confirm`. It does **not**
call `endMeeting`, so nothing blocks a second upload. Combined with the merged
backend change (clips instead of overwrite), re-recording the same calendar
meeting already produces two clips.

**Consuming path is single-recording.** `MeetingDetailView.vue` renders one
`RecordingAudioPlayer` (`getMeetingAudio` → `GET /meeting-notes/{id}/audio`,
one blob) and one whole-meeting transcript. It has no notion of clips.

**Conclusion:** the producing path needs *verification + a test*, not new
behavior. The substantive work is making the consuming UI clip-aware.

## 3. Goals / non-goals

**Goals**
- Surface a meeting's multiple clips in oats's `MeetingDetailView` with
  web-ui parity: stacked per-clip players, per-clip transcript switching,
  host-only per-clip delete.
- Keep single-clip and legacy meetings visually identical to today.
- Lock in the producing behavior with a test.
- Keep the cloud/offline backend abstraction intact.

**Non-goals**
- Local (offline) multi-clip — PR 2 (§8).
- Speaker / voice-print reconciliation across clips (documented backend
  limitation).
- Any change to the recording/upload behavior.

## 4. Design

### 4.1 API / plumbing layer

`src/composables/useMeetingApi.ts`
- Add `interface MeetingAudioClip { transcript_id: string; duration_ms: number | null; created_at: string; legacy: boolean }`.
- Parse `audio_clips` off the `GET /meeting-notes/:id` response (backend
  guarantees ≥1 entry when audio exists; a pre-clip meeting yields a single
  `{ transcript_id: 'legacy', legacy: true }` entry).
- Add `transcript_id: string` to `TranscriptChunk` (the transcript endpoint
  already tags every chunk; legacy chunks come back as `'legacy'`).
- Add `deleteMeetingRecordingClip(meetingId, transcriptId)` →
  `DELETE /meeting-notes/:id/recording/:transcriptId` (expects 200).

`src/tauri.ts` + `src-tauri/src/commands.rs`
- Extend the `fetch_meeting_audio` command and its `tauri.ts` wrapper with an
  optional `transcriptId`. Present → `GET /meeting-notes/:id/audio/:transcriptId`;
  omitted → today's `/audio` (legacy). Rust unit test covers URL construction.

### 4.2 Backend abstraction — `src/composables/useBackend.ts`

- `MeetingDetail` gains `audioClips: MeetingAudioClip[]` (empty when none).
- Ariso `getMeetingDetail` maps `data.audio_clips`.
- `getMeetingAudio(item, transcriptId?)`: Ariso routes to the per-clip endpoint
  when `transcriptId` is a real (non-legacy) id; Local **ignores** `transcriptId`
  and returns its single recording (unchanged until PR 2).
- New `deleteMeetingClip(item, transcriptId)` on the `Backend` interface: Ariso
  calls `deleteMeetingRecordingClip`; Local throws an "unsupported" error for
  now (delete lands with PR 2).

### 4.3 UI — `src/views/MeetingDetailView.vue`

- **Stacked players.** Replace the single `RecordingAudioPlayer` with one per
  clip (oldest-first, the `audio_clips` order). Label `Recording N · Mm SSs`
  (drop `· Mm SSs` when `duration_ms` is null). Each player's `load` resolves
  its own clip via `getMeetingAudio(item, clip.transcript_id)`.
- **Per-clip transcript switching.** Add `activeClipId` (default = first clip).
  Fetch the meeting transcript once and **partition chunks by `transcript_id`
  client-side**; the transcript pane renders the active clip's chunks. Clicking
  a clip sets it active. Client-side partitioning avoids web-ui's async
  request-sequence race entirely (no in-flight token needed).
- **Host-only per-clip delete.** oats has **no delete UI today** (no
  whole-recording delete, no generic confirm dialog — only
  `AriJoinConfirmDialog`), so this introduces oats's first destructive action.
  When there is more than one non-legacy clip **and** `isHost` (already computed
  at `MeetingDetailView.vue:326`), render a delete button per clip → a new
  confirm dialog modeled on `AriJoinConfirmDialog` → `deleteMeetingClip` →
  refetch detail. If the deleted clip was active, reset active to the first
  remaining clip. Single-clip / legacy meetings show no delete (unchanged from
  today) — this matches web-ui's `showPerClipDelete` gate (>1 clip) without
  oats needing a whole-recording delete.
- **Degradation.** 1 clip or legacy → identical to today (one player, whole
  transcript, single delete button). 0 clips but `hasTranscript` (imported
  transcript) → no players, whole transcript as now. Per-clip audio 404 → the
  player's existing "No audio" state.

### 4.4 Producing path — verify only

No code change expected. Add a unit test asserting a second recording of the
same calendar meeting resolves the same `meetingId` (through
`resolveAssociation`) and that `finalizeRecording`/`uploadAudio` carries it to
the `…/{id}/audio/presign` path, so the two-clip outcome is pinned.

## 5. Data flow

1. Open detail → `getMeetingDetail` returns `audioClips` + (on transcript-tab
   open) the tagged transcript chunks.
2. Render one player per clip; partition chunks by `transcript_id`; active =
   `audioClips[0]`.
3. Click a clip → set `activeClipId` → transcript pane shows that clip's chunks.
4. Host clicks delete on a clip → confirm → `DELETE …/recording/:transcriptId`
   → refetch detail → players + transcript re-derive; active resets if needed.

## 6. Error handling / edge cases

- Meeting with transcript but no clips (imported) → whole-transcript fallback.
- Legacy meeting → single player via `/audio`; transcript unpartitioned.
- Per-clip audio 404 → "No audio" (existing behavior).
- Delete of the active clip → active resets to first remaining clip.
- Non-host viewer → no delete buttons.
- Local backend → single player, no per-clip delete (unsupported).

## 7. Testing

- **Vitest** (`MeetingDetailView`, `useMeetingApi`): N players for N clips;
  transcript partition by clip; host-gated delete visibility; single/legacy
  fallback; delete → refetch. ⚠️ `MeetingDetailView` Vitest files are brittle in
  full runs (incomplete `../tauri` mock + jsdom/TipTap gaps) — verify in
  isolation.
- **Rust** (`cargo test`, with the documented `DYLD_LIBRARY_PATH` +
  `--test-threads=1` workaround): `fetch_meeting_audio` per-clip URL
  construction.

## 8. Follow-up — Local (offline) backend (PR 2, separate spec)

Local has no calendar; the resume trigger is a **time window since last stop**
(if a new recording starts within N minutes of the previous stopping, treat it
as the same meeting). Rather than modelling N recording dirs, PR 2 **appends the
new clip's audio to the existing recording and stitches its transcript onto the
existing transcript**, then regenerates notes **incrementally**. That work — the
on-disk append, transcript stitching, and incremental notes — gets its own
brainstorm → spec → plan cycle and does not block this PR.
