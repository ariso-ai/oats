# Local speaker rename (issue #409)

## Problem

Local-backend recordings diarize speakers into generic labels — `**Speaker
1**`, `**Speaker 2**` — and there is no way to turn those into real names.
`storage.rs::render_markdown` writes those labels straight into
`transcript.md` (`storage.rs:417-459`), and `src/views/localTranscript.ts`
parses them back out for the Transcript tab, so every local transcript a host
looks at just says "Speaker 1", forever.

Ariso already solves this for cloud recordings:
`src/composables/useSpeakerAssignment.ts` and
`src/views/SpeakerAssignPopover.vue` let a host assign a diarized speaker to a
real person, searched from the org directory or auto-suggested from server
side voice matching. Both are explicitly gated on `!detail.isLocal`
(`MeetingDetailView.vue:658-665`) because they depend on infrastructure that
only exists server-side. Local mode has no org directory and no voice-print
matching, so it needs its own, much simpler UI: manual free-text rename, no
search, no auto-match, no voice playback.

## Goal

A host viewing a local recording can rename any diarized speaker (e.g.
"Speaker 1" → "Priya" or an email address) from the Meeting Detail view. The
new label:

- Persists to disk (`meta.json` and `segments.json`), surviving app restarts
  and future clip appends.
- Shows up immediately in the Transcript tab, replacing the old label
  everywhere it appeared, with the transcript text itself untouched.

Reviewable as: open a local recording with two or more diarized speakers,
rename "Speaker 2" to "Priya" via the new Speakers affordance, see the
Transcript tab immediately show "Priya" instead of "Speaker 2" on every line
that speaker has, and confirm `~/.ariso/recordings/<id>/meta.json` and
`segments.json` both carry the new label after reopening the app.

## Non-goals

- **Cloud (Ariso) meetings.** Unaffected; they keep the existing
  `useSpeakerAssignment`/`SpeakerAssignPopover` flow. This spec adds a
  parallel, local-only surface rather than touching that one.
- **Voice search, auto-match, or voice-sample playback.** Those need the
  server's voice matching and org directory (`SpeakerAssignPopover.vue`'s
  Play/search/confidence UI). Local mode gets a plain text field per speaker.
- **Remembering a speaker's name or voice across recordings.** The mapping is
  per-recording, stored in that recording's own `meta.json`/`segments.json`.
  Building a local voice-profile store that recognizes "this is the same
  Priya as last week" would be a much bigger feature with its own privacy
  trade-offs (it starts to resemble the cloud auto-match feature this app
  keeps server-side on purpose), and nothing in the request asks for it.
- **Automatically rewriting already-generated AI notes text.** See
  [Notes: labels can go stale](#notes-labels-can-go-stale) and
  [Open questions](#open-questions).
- **Renaming recordings that predate `segments.json`.** See
  [Error handling](#error-handling).
- **Duplicate-name validation.** Two speakers can intentionally share a label
  (e.g. diarization split one person across two speaker indices — labeling
  both "Priya" is the whole point, not a mistake), so nothing blocks it. This
  mirrors the existing local title-rename design's stance of no duplicate
  checks (`2026-06-11-local-meeting-rename-design.md`).

## Design

### Where the label actually lives

`RecordingMeta.participants: Vec<Participant>` (`storage.rs:57-61, 96`) is
already the source of truth `render_markdown` reads before falling back to
`Speaker {n+1}` (`storage.rs:419-425`). It is populated once, at finalize
time, from diarization (`transcript_normalization.rs:89-94`, default labels
`"Speaker {id+1}"`) and mirrored into `segments.json`'s own
`participants: Vec<Participant>` (`transcribe.rs:444-451`). The two lists are
kept in lockstep already — `next_speaker_offset`/`offset_participants`
(`storage.rs:461-489`) shift both consistently when a clip is appended
(`transcribe.rs:615-624`). Renaming a speaker is therefore a matter of editing
one `Participant.label` in both files by matching on `id` (stable across
appends — never array position) and re-running the existing
`render_markdown` to rewrite `transcript.md`. No markdown surgery, no new
storage format.

### Rust: two new commands in `commands.rs`

```rust
/// List a local recording's diarized speakers (id + current label), read
/// from `meta.json`. Empty for a recording with no diarized speech.
#[tauri::command]
pub fn list_local_speakers(id: String) -> Result<Vec<crate::storage::Participant>, String> {
    let dir = recording_dir(&id)?;
    Ok(crate::storage::read_meta(&dir)?.participants)
}

/// Maximum speaker label length, in characters. Generous vs. `MAX_TITLE_CHARS`
/// (40) since a label may hold a full name plus an email address.
const MAX_SPEAKER_LABEL_CHARS: usize = 60;

/// Rename one diarized speaker in a local recording: updates the label in
/// `meta.json` and `segments.json`, then re-renders `transcript.md` so the
/// change is visible immediately. Returns the re-rendered transcript so the
/// caller can patch its in-memory copy without a re-read round trip (mirrors
/// `set_vault_task_done`).
#[tauri::command]
pub fn rename_local_speaker(id: String, speaker_id: u32, label: String) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("label must not be empty".to_string());
    }
    if label.chars().count() > MAX_SPEAKER_LABEL_CHARS {
        return Err(format!("label must be {MAX_SPEAKER_LABEL_CHARS} characters or fewer"));
    }
    let dir = recording_dir(&id)?;
    let mut meta = crate::storage::read_meta(&dir)?;
    let participant = meta
        .participants
        .iter_mut()
        .find(|p| p.id == speaker_id)
        .ok_or_else(|| format!("unknown speaker id: {speaker_id}"))?;
    participant.label = label.to_string();

    // segments.json is what render_markdown's caller re-derives the markdown
    // from; recordings from before it existed can't be re-rendered here.
    let mut segments = crate::storage::read_segments(&dir)?
        .ok_or_else(|| "this recording predates structured transcripts and can't be renamed".to_string())?;
    if let Some(p) = segments.participants.iter_mut().find(|p| p.id == speaker_id) {
        p.label = label.to_string();
    }
    crate::storage::write_segments(&dir, &segments)?;
    crate::storage::write_meta(&dir, &meta)?;

    let md = crate::storage::render_markdown(&meta, &segments.segments);
    crate::storage::write_transcript(&dir, &md)?;
    Ok(md)
}
```

Both registered in `main.rs`'s `generate_handler![...]` list, alongside
`commands::rename_local_recording`. Neither needs a capability entry — like
every other `commands::*` function, they're plain `#[tauri::command]`s, not
plugin commands (confirmed for `rename_local_recording`: it appears in no
`capabilities/*.json`).

### TypeScript: `src/tauri.ts`

```ts
/** List a local recording's diarized speakers (id + current label). */
listSpeakers(id: string): Promise<Array<{ id: number; label: string }>> {
  return invoke('list_local_speakers', { id });
},
/** Rename one diarized speaker. Resolves to the re-rendered transcript
 *  markdown so the caller can update its copy in place. */
renameSpeaker(id: string, speakerId: number, label: string): Promise<string> {
  return invoke<string>('rename_local_speaker', { id, speakerId, label });
},
```

alongside the existing `local.*` wrappers.

### `useBackend.ts`: one new field, no new interface method

`MeetingDetail` gains one field in its "Local-recording fields" group
(`useBackend.ts:184-191`):

```ts
/** Local: this recording's diarized speakers (id + current label). Empty for
 *  Ariso meetings and for local recordings with no diarized speech. */
localSpeakers: Participant[];
```

`LocalBackend.getMeetingDetail` (`useBackend.ts:677-706`) fetches it alongside
the existing note/transcript reads, gated the same way (only meetings with a
transcript can have speakers):

```ts
const [note, transcript, localSpeakers] = await Promise.all([
  item.files?.hasNote ? local.readRecordingFile(item.id, 'note').catch(() => null) : Promise.resolve(null),
  item.files?.hasTranscript ? local.readRecordingFile(item.id, 'transcript').catch(() => null) : Promise.resolve(null),
  item.files?.hasTranscript ? local.listSpeakers(item.id).catch(() => []) : Promise.resolve([]),
]);
```

`ArisoBackend.getMeetingDetail` sets `localSpeakers: []`.

This does **not** go on the shared `Backend` interface as a `renameSpeaker`
method. Ariso's speaker assignment isn't "the same action, different
backend" — it's a fundamentally different flow (identity search against an
org directory, not a label string), which is exactly why
`useSpeakerAssignment` already bypasses `Backend` and calls `useMeetingApi`
directly. The local composable follows the same precedent and calls
`api.local.renameSpeaker` directly.

### Frontend: a local-only composable and popover

New `src/composables/useLocalSpeakerRename.ts`, deliberately much smaller
than `useSpeakerAssignment.ts` — no search, no debounce, no voice sample
playback, no auto-match ranking:

```ts
export function useLocalSpeakerRename(deps: {
  recordingId: Ref<string | null>;
  speakers: Ref<Participant[]>;      // writable computed into detail.value.localSpeakers
  transcript: Ref<string | TranscriptChunk[] | null>; // writable computed into detail.value.transcript
}) {
  const open = ref(false);
  const editingId = ref<number | null>(null);
  const draft = ref('');
  const saving = ref(false);
  const error = ref<string | null>(null);
  // startEdit(id), cancelEdit(), commit() -> api.local.renameSpeaker(...),
  // then speakers.value and transcript.value are patched from the result so
  // the Transcript tab (which re-parses detail.transcript via
  // parseLocalTranscript) picks up the new label without a re-read.
}
```

New `src/views/LocalSpeakerRenamePopover.vue`: one row per speaker, each
showing the current label with a click-to-edit text input (Enter commits,
Escape cancels — matching the title rename UX precedent), a save error shown
inline like `SpeakerAssignPopover`'s own error line.

### `MeetingDetailView.vue` wiring

A second trigger sits beside the existing Ariso-only one, mutually exclusive
since `isLocal` and `!isLocal` never both hold:

```ts
const canRenameLocalSpeakers = computed(
  () => !!detail.value?.isLocal && (detail.value.localSpeakers?.length ?? 0) > 0
);
```

Rendered in the same `meta-item` slot the Ariso "Speakers" chip occupies
(`MeetingDetailView.vue:141-158`), so the affordance always lives in the same
place regardless of backend — same trigger styling, same anchor/popover
positioning code (`onSpeakersClick`/`speakersAnchor`), branching on `isLocal`
to decide which popover mounts.

### Notes: labels can go stale

AI notes for local recordings are generated by feeding the already-rendered
`transcript.md` — labels baked into prose — to the on-device LLM
(`transcribe.rs::process_notes`, `transcript.rs:247-269`). Renaming a speaker
re-renders `transcript.md` but does **not** touch an already-generated
`ari-note.md`; existing notes keep whatever label was current when they were
generated. This is unlike the title-rename precedent, where the title isn't
fed into notes generation at all and so can't go stale.

The app already has an affordance for this: "Regenerate notes"
(`MeetingDetailView.vue:207-216`, `local.retryNotes`), which re-runs
`process_notes` against the now-updated `transcript.md`. This spec relies on
that existing action rather than building new plumbing — see
[Open questions](#open-questions) for whether that's the right default.

## Cloud vs offline

Local only. Ariso's speaker assignment already exists and is untouched by
this design. The local design is possible specifically because
`transcript.md` is a from-scratch render the app fully controls
(`render_markdown`); Ariso transcripts are structured chunks served by the
backend, and Ariso's speaker identity model (org user / email / display name)
has no offline equivalent — hence the separate, simpler local surface rather
than extending the shared one.

Nothing here makes a network call; it's a local file edit, consistent with
the offline-mode privacy guarantee.

## Error handling

- **Empty or over-length label**: `rename_local_speaker` returns an error
  before touching disk; the popover shows it inline and leaves the input
  open, mirroring the local-rename-title precedent's error handling.
- **Unknown `speaker_id`**: shouldn't happen through the UI (ids come from
  `list_local_speakers`), but the command still rejects it — defense in
  depth, same posture as `rename_local_recording`'s length check.
- **Recording predates `segments.json`** (transcribed before structured
  segments were introduced): `rename_local_speaker` returns "this recording
  predates structured transcripts and can't be renamed" rather than doing
  partial markdown surgery. The popover surfaces this as a normal inline
  error; it's rare (only recordings from before the structured-transcript
  feature shipped) and out of scope to backfill.
- **Write failure mid-command** (disk full, permissions): `write_segments`/
  `write_meta`/`write_transcript` each use `storage::write_atomic`
  (write-to-temp-then-rename), so a failure at any step leaves the
  previously-committed files intact; the command surfaces the error and nothing
  half-written is visible. A failure between `write_meta` succeeding and
  `write_transcript` failing leaves `meta.json`'s label ahead of
  `transcript.md`'s until the user retries — the retry is idempotent (same
  label, same write), so this self-heals on the next attempt.

## Testing

- **Rust (`commands.rs`, `storage.rs`)**: `rename_local_speaker` happy path
  (label updates in both `meta.json` and `segments.json`, `transcript.md`
  re-rendered with the new label, unrelated segment text unchanged); rejects
  empty and over-length labels without touching disk; rejects an unknown
  `speaker_id`; returns the "predates structured transcripts" error when
  `segments.json` is absent; a label containing quotes round-trips through
  `render_markdown`'s existing `esc()` escaping. `list_local_speakers` returns
  `meta.participants` verbatim, empty list for a recording with none.
- **Vitest (`MeetingDetailView.test.ts`)**: the local Speakers chip is absent
  for Ariso meetings and for a local meeting with no diarized speakers; for a
  local meeting with speakers, clicking it opens the rename popover listing
  `detail.localSpeakers`; committing a rename calls
  `local.renameSpeaker(id, speakerId, label)` and, on success, both the
  Transcript tab (re-parsed `detail.transcript`) and the popover's own list
  show the new label immediately; a rejected rename surfaces its error inline
  and leaves the input open; Enter/Escape behave like the title-rename
  precedent.
- **Manual**: record locally with two distinguishable voices, wait for
  transcription, rename "Speaker 2" via the new chip, confirm the Transcript
  tab updates immediately and `~/.ariso/recordings/<id>/meta.json` +
  `segments.json` both show the new label after quitting and relaunching the
  app. Separately confirm an already-generated note keeps the old label until
  "Regenerate notes" is clicked, after which the freshly generated note uses
  the new one.

## Open questions

- **Should a rename touch already-generated AI notes?** This spec's default
  is no — notes go stale until the user hits the existing "Regenerate notes"
  button, which is zero new code and never silently rewrites LLM-authored
  prose. The alternative (a literal find/replace of the old label string
  across `ari-note.md`) is mechanical and would satisfy "consistently...in
  notes" more literally, but "Speaker 1" is far more collision-prone as
  plain text than as a document header (e.g. a note discussing "the first
  speaker" or literally being about a person later renamed to something that
  reads as prose), so the risk is misleading edits to text the app is
  presenting as an accurate LLM output. Confirm — this is the one decision
  that would add real scope back into this spec.
