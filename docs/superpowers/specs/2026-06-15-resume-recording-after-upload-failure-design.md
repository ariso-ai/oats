# Resume recording after an upload failure

**Date:** 2026-06-15
**Branch:** feat/error-path-audio-upload
**Status:** Approved (design)

## Problem

When a recording's upload fails, the pill (`WaveformView.vue`) shows an error
state (✗) with two controls:

- **Retry** (↻) — re-uploads the held blob via `runFinalize()`.
- **Dismiss** (✕) — `dismissFailed()` deletes the on-disk pending buffer and
  closes the window. The captured audio is **lost**.

There is no way to recover from a failed upload by recording more and trying
again. A user whose upload failed (e.g. transient network loss) must either
keep retrying the exact same blob or throw the recording away.

## Goal

From the failed pill, let the user **resume recording**. The newly captured
audio is appended to the previously-captured (failed) audio, and stopping again
uploads the **combined** recording as a single recording/meeting. A discard
affordance is retained so a failed recording can still be thrown away.

## Approach

**In-memory blob concatenation at the next finalize (Approach A).**

The failed blob is already held in `stoppedBlob` / `stoppedMeta` refs. On
resume we keep those refs, restart the mic into a fresh `mp3Chunks` buffer
(existing `useRecorder.startRecording()` behavior), and on the next stop we
concatenate `new Blob([stoppedBlob, newBlob])` before finalizing.

Concatenated MP3 frames play back correctly — each segment is a self-contained
MP3 stream, and this is the same concatenation strategy `pending.combine`
already uses for multi-segment resumable uploads. No new Rust/disk plumbing is
required.

**Rejected — Approach B (persist-and-combine on disk):** write each segment to
the pending store and merge via `pending.combine([...keys])` before upload.
More durable across a crash mid-second-segment, but heavier (extra disk
round-trips, per-segment keys, more moving parts) for a marginal gain. The
first segment is already crash-safe on disk; that matches today's "audio is
persisted before upload" guarantee.

## UI changes (`WaveformView.vue`)

The failed-state template (`v-if="uploadResult === 'failed'"`, lines 14–26)
gains a third control. The three buttons become:

| Control | Icon | Handler | Behavior |
|---|---|---|---|
| Retry | ↻ | `runFinalize` | Unchanged — re-upload the held blob. |
| Resume | ⏺ | `resumeFailed` (new) | Clear `uploadResult`, keep the held blob/meta, restart the mic; pill returns to the live recording view. |
| Discard | ✕ | `dismissFailed` (existing) | Delete the on-disk buffer and close. (The `.dismiss-btn` class/handler is unchanged; only its position/label context shifts from "the dismiss action" to "discard".) |

Layout: three stacked `ctrl-btn`s in the narrow pill. Add a `.resume-btn` style
alongside the existing `.retry-btn` / `.dismiss-btn`. A record-dot glyph (●) in
a recording-red tone distinguishes Resume from Retry.

## Behavior changes (`WaveformView.vue`)

### `resumeFailed()` (new)

```
async function resumeFailed() {
  if (!stoppedBlob.value) return;        // nothing to resume onto
  uploadResult.value = null;             // leave the failed state → recording view
  closedSent stays false (window never closed)
  await startRecording();                // fresh mic capture into a new mp3Chunks
}
```

`startRecording()` resets `recorder.durationSeconds` to 0, so the resumed
segment's timer is **segment-local** (starts at 00:00). This is the chosen
behavior — simplest, and the summed total is what ultimately gets uploaded.

### `handleStop()` append (lines 334–379)

After obtaining the new `mp3Blob`, if a prior `stoppedBlob` exists, fold it in
**before** setting the refs and finalizing:

```
const newBlob = await recorder.stopRecording();
const prevBlob = stoppedBlob.value;
const prevMeta = stoppedMeta.value;

const combinedBlob = prevBlob
  ? new Blob([prevBlob, newBlob], { type: 'audio/mpeg' })
  : newBlob;

if (combinedBlob.size > 0 && backend.value) {
  stoppedBlob.value = combinedBlob;
  stoppedMeta.value = {
    startAt: prevMeta?.startAt ?? startAt,                 // keep the ORIGINAL start
    endAt,                                                  // new end
    durationSeconds: (prevMeta?.durationSeconds ?? 0)
                     + recorder.durationSeconds.value,      // summed
    meetingId: prevMeta?.meetingId
               ?? effectiveMeetingId.value ?? undefined,
  };
  await runFinalize();
}
```

Keeping the original `startAt` means:

- `finalizeRecording` re-keys the same on-disk buffer (`createdAt = startAt`),
  overwriting it with the fuller audio rather than orphaning the first segment.
- The local backend's deterministic recording id
  (`localRecordingIdFromStart(startAt)`) stays pinned to the same Library row /
  red dot.

This folds across N rounds: each stop concatenates the accumulated blob with
the latest segment, and each failed finalize leaves the full accumulation in
`stoppedBlob` for the next resume.

### Guards / edge cases

- **Resume guard:** `resumeFailed` no-ops if `stoppedBlob` is null.
- **`isStopping` reset:** `handleStop` sets `isStopping = true` and never
  resets it (today the window always closes). With resume, a second stop must
  work, so `resumeFailed` must reset `isStopping.value = false` (and clear any
  lingering `closeTimer`) when re-entering recording.
- **Empty resumed segment:** if the resumed segment produces a zero-size blob
  but a prior blob exists, still finalize the prior blob (don't drop it) — the
  `combinedBlob.size > 0` check covers this since `prevBlob` is non-empty.
- **Silence backstop & confirm overlay:** unaffected — they key off
  `recorder.isRecording` / `confirmVisible`, both correct after a fresh
  `startRecording()`.

## Out of scope

- Changing the pending-uploads / Library recovery path.
- Visually offsetting the resumed timer to show a running total.
- Persisting the in-progress resumed segment before stop (crash-durability of
  Approach B).

## Testing (`WaveformView.test.ts`)

Match the existing fake-timer + mocked-composable style. New/updated cases:

1. **Failed pill shows Retry, Resume, and Discard controls** (extends the
   existing "failed upload shows Retry and Dismiss controls" test).
2. **Resume clears the failed state and re-enters recording** while preserving
   the held blob: after a failed stop, clicking Resume calls
   `startRecording` again and the pill leaves the `.status-icon.err` state
   (no `closeWin`, no `discardPendingAudio`).
3. **Stop after resume uploads a combined blob with the original startAt and
   summed duration:** mock two `stopRecording` blobs; assert the second
   `finalizeRecording` call receives a blob whose size equals the sum of both
   segments and meta `{ startAt: <original>, durationSeconds: <sum> }`.
4. **Discard still deletes the buffer and closes** (existing "Dismiss discards
   the buffered audio and closes the window" test — keep green; selector may
   move from `.dismiss-btn` if reclassed, but behavior is unchanged).

## Files touched

- `src/views/WaveformView.vue` — template (3 buttons), `resumeFailed`,
  `handleStop` append, `isStopping` reset, `.resume-btn` style.
- `src/views/WaveformView.test.ts` — new/updated cases above.
