# Download Audio Button — Design Spec

**Date:** 2026-11-05
**Issue:** #465
**Branch:** autopilot/issue-465

---

## Goal

Add a "Download audio" button in the meeting detail view for local-backend recordings,
placed next to the existing "Download" (transcript) button. Users can export the raw
recorded audio to their filesystem in one click.

## Non-Goals

- Cloud/Ariso recordings: audio lives on the server and is already streamed for
  playback; exporting it is a separate concern and out of scope here.
- Transcoding: the audio is copied byte-for-byte, same as the transcript export.
- Any change to recording, transcription, or notes pipelines.

## Constraints

- Local-only feature, gated the same way `showDownloadTranscript` is.
- Capabilities files (`src-tauri/src/capabilities/`) cannot be changed.
  `library-export-dialog.json` already grants `dialog:allow-save` to the library window
  for the existing transcript save dialog, so no capability change is needed.
- `package.json` / `Cargo.lock` cannot be changed.

---

## Decisions

| # | Question | Answer | Source |
|---|----------|--------|--------|
| 1 | Which tab should the button appear on? | Transcript tab — that's where audio playback and transcript download already live | Default |
| 2 | What gate controls button visibility? | `detail.hasAudioRecording` (mirrors `hasTranscript` for transcript download) | Default |
| 3 | What default filename does the save dialog suggest? | `{title} audio - {date}.mp3` (same pattern as transcript) | Default |
| 4 | Separate error display for audio download failure? | Yes, a dedicated `audioDownloadError` ref, mirroring `transcriptDownloadError` | Default |
| 5 | Extend `copy_recording_file` with an `audio` kind, or add a new command? | New command `copy_recording_audio` — audio path resolution is distinct (vault attachment vs legacy `recording.mp3`) from text-file resolution; keeping the APIs separate is cleaner and avoids touching the validated `note_or_transcript_filename` whitelist | Default |

---

## Interfaces

### New Rust command: `copy_recording_audio`

```rust
#[tauri::command]
pub fn copy_recording_audio(id: String, dest: String) -> Result<(), String>
```

- Validates `id` via `storage::validate_recording_id` (same as all recording commands).
- Resolves the audio path exactly as `read_recording_audio_bytes` does:
  - New recordings: `vault_root() / Attachments / meta.audio_file`
  - Legacy recordings (`audio_file: None`): `recording_dir(id) / recording.mp3`
- Requires `dest` to be an absolute path.
- Guards against same-file (src == dest) overwrite using `same_file::is_same_file`.
- Copies atomically via a `.tmp` sibling → `rename`, same as `copy_recording_file`.

### New TypeScript wrapper: `local.copyRecordingAudio`

```typescript
copyRecordingAudio(id: string, dest: string): Promise<void>
```

### New TypeScript helper: `pickAudioSavePath`

```typescript
export async function pickAudioSavePath(defaultName: string): Promise<string | null>
```

Same as `pickMarkdownSavePath` but filters for `.mp3`.

### New filename helper: `audioFilename`

```typescript
export function audioFilename(title: string | undefined, startAt: string | undefined): string
```

Returns `{sanitizedTitle} audio - {YYYY-MM-DD}.mp3` (or without date if `startAt` is
absent), mirroring `transcriptFilename`.

---

## UI Changes (`MeetingDetailView.vue`)

### Template

Add a "Download audio" button immediately before the existing "Download" (transcript)
button in `.card-tabs`. Both buttons share the `.tab-download` CSS class:

```html
<button v-if="showDownloadAudio" class="tab-download" type="button"
        :disabled="downloadingAudio" title="Download audio"
        @click="onDownloadAudioClick">
  <svg .../>
  Download audio
</button>
<button v-if="showDownloadTranscript" class="tab-download" ...>
  Download
</button>
```

Add an error display in the transcript pane right after the existing
`transcriptDownloadError` paragraph.

### New computed / refs

```typescript
const showDownloadAudio = computed(
  () =>
    !!detail.value?.isLocal &&
    activeTab.value === 'transcript' &&
    !!detail.value?.hasAudioRecording
);
const downloadingAudio = ref(false);
const audioDownloadError = ref<string | null>(null);
```

### CSS addition

```css
.tab-download ~ .tab-download { margin-left: 0; }
```

Prevents the second download button from claiming its own `margin-left: auto` when both
buttons are shown together.

---

## Acceptance Criteria

- [ ] A "Download audio" button appears next to the existing "Download" (transcript)
      button when viewing the Transcript tab of a local recording that has audio.
- [ ] The button is not shown for Ariso/cloud recordings.
- [ ] The button is not shown when the recording has no audio (`hasAudioRecording` is
      false).
- [ ] The button is not shown on tabs other than Transcript.
- [ ] Clicking the button opens a native save dialog pre-filled with
      `{title} audio - {date}.mp3`.
- [ ] After the user confirms a save path, the audio file is copied byte-for-byte to that
      path.
- [ ] If the copy fails, an inline error message is shown in the transcript pane.
- [ ] While the save dialog or copy is in progress, the button is disabled (guards the
      same `reqId` / meeting-switch pattern as the transcript download).
- [ ] `npm test` passes.
- [ ] `npm run vite:build` succeeds.
- [ ] `cargo build --locked` succeeds.

---

## Test Strategy

### Frontend unit tests (`transcriptDownloadName.test.ts`)

Add `audioFilename` test cases covering: normal title + date, missing date, empty title.

### Rust unit tests (`commands.rs`)

Mirror the existing `copy_recording_file_*` tests:
- `copy_recording_audio_copies_bytes` — byte-for-byte copy of a new-style recording's
  vault audio.
- `copy_recording_audio_copies_legacy_bytes` — byte-for-byte copy of a legacy
  `recording.mp3`.
- `copy_recording_audio_refuses_to_copy_onto_the_source` — same-file guard.
- `copy_recording_audio_rejects_relative_destination` — absolute path guard.
- `copy_recording_audio_errors_when_audio_missing` — error when audio file doesn't exist.

### Cannot verify at this layer

- End-to-end button visibility and click flow in the running app (requires the live
  desktop app with a real recording).
- Native save dialog behavior (Tauri plugin, requires the running app).
