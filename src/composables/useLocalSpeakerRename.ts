import { computed, ref, type ComputedRef, type Ref, type WritableComputedRef } from 'vue';
import { local, type Participant } from '../tauri';

/** Mirrors `MAX_SPEAKER_LABEL_CHARS` in `src-tauri/src/commands.rs`. The UI
 *  validates first so the common mistake never round-trips; the backend check
 *  is defense in depth. */
export const MAX_SPEAKER_LABEL_CHARS = 60;

export interface LocalSpeakerRename {
  open: Ref<boolean>;
  editingId: Ref<number | null>;
  draft: Ref<string>;
  saving: Ref<boolean>;
  error: Ref<string | null>;
  speakers: ComputedRef<Participant[]>;
  startEdit(speakerId: number): void;
  cancelEdit(): void;
  commit(): Promise<void>;
  reset(): void;
}

/**
 * Rename a diarized speaker on a *local* recording — a plain label edit, not
 * the Ariso identity assignment `useSpeakerAssignment` does. Offline there is
 * no org directory and no voice matching, so there is nothing to search or
 * auto-match: the whole surface is one text field per speaker.
 *
 * Like `useSpeakerAssignment`, this bypasses the shared `Backend` interface and
 * talks to its own API layer directly — the two flows aren't one action with
 * two implementations, they're different features that happen to sit in the
 * same chip.
 */
export function useLocalSpeakerRename(deps: {
  recordingId: Ref<string | null>;
  /** Writable computed onto `detail.localSpeakers` — a successful rename
   *  patches it so the panel's own list updates without a re-read. */
  speakers: WritableComputedRef<Participant[]>;
  /** Writable computed onto `detail.transcript` (local markdown). The command
   *  returns the re-rendered transcript, so the Transcript tab — which parses
   *  this — picks up the new label immediately. */
  transcript: WritableComputedRef<string | undefined>;
}): LocalSpeakerRename {
  const open = ref(false);
  const editingId = ref<number | null>(null);
  const draft = ref('');
  const saving = ref(false);
  const error = ref<string | null>(null);

  const speakers = computed(() => deps.speakers.value);

  // Bumped by `reset()` to disown any request still in flight. The recording-id
  // checks below can't catch a close-and-reopen on the *same* recording: the id
  // still matches, so a stale result would patch over a newer edit session and,
  // worse, its `finally` would clear `saving` while a second rename is still
  // pending — re-enabling the input mid-save.
  let generation = 0;

  function startEdit(speakerId: number): void {
    error.value = null;
    editingId.value = speakerId;
    draft.value = deps.speakers.value.find((p) => p.id === speakerId)?.label ?? '';
  }

  function cancelEdit(): void {
    editingId.value = null;
    draft.value = '';
    error.value = null;
  }

  function reset(): void {
    generation++;
    open.value = false;
    saving.value = false;
    cancelEdit();
  }

  async function commit(): Promise<void> {
    const id = deps.recordingId.value;
    const speakerId = editingId.value;
    if (!id || speakerId === null || saving.value) return;

    const label = draft.value.trim();
    // Validated here as well as in Rust so a typo doesn't cost a round trip —
    // and so the message is phrased for a person ("Name"), not for the command.
    if (!label) {
      error.value = 'Name must not be empty';
      return;
    }
    if ([...label].length > MAX_SPEAKER_LABEL_CHARS) {
      error.value = `Name must be ${MAX_SPEAKER_LABEL_CHARS} characters or fewer`;
      return;
    }

    saving.value = true;
    error.value = null;
    const myGeneration = generation;
    try {
      const markdown = await local.renameSpeaker(id, speakerId, label);
      // Disowned by a `reset()` that happened while this was in flight.
      if (myGeneration !== generation) return;
      // The displayed recording can change while this request is in flight
      // (user picks a different meeting before it resolves). Applying a
      // stale result would clobber the new recording's speakers/transcript
      // with the old one's — drop it on the floor instead. Same idiom as the
      // reqId guard in MeetingDetailView's reloadLocalArtifact.
      if (deps.recordingId.value !== id) return;
      // Patched from the command's own result rather than re-read: the label we
      // sent and the transcript it rendered are the authoritative pair.
      deps.speakers.value = deps.speakers.value.map((p) =>
        p.id === speakerId ? { ...p, label } : p
      );
      deps.transcript.value = markdown;
      // Only close the edit field if it's still on the speaker we just
      // renamed — the user may have already opened a different speaker's
      // edit while this request was in flight, and that shouldn't be clobbered.
      if (editingId.value === speakerId) {
        editingId.value = null;
        draft.value = '';
      }
    } catch (e) {
      if (myGeneration !== generation) return;
      // Same stale-meeting guard as the success path above. `load()` calls
      // reset() on a meeting switch, so without this a rename that failed on
      // meeting A would paint A's error banner into the panel the next time it
      // is opened on meeting B.
      if (deps.recordingId.value !== id) return;
      // A Tauri command rejects with a bare string, so this keeps the backend's
      // own wording (e.g. the "predates structured transcripts" case).
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      // Generation-guarded, but deliberately NOT id-guarded. An id guard here
      // could strand `saving` at `true` and disable the field for good, which
      // is worse than the transient it prevents. A generation bump can't:
      // `reset()` is the only thing that bumps it, and it clears `saving`
      // itself — so skipping this write is exactly what keeps a disowned
      // request from clearing `saving` out from under a newer one.
      //
      // Written as a conditional rather than an early `return`: a `return` in
      // `finally` discards whatever completion is pending, which is harmless
      // today (every throw is caught above, and both paths return `undefined`)
      // but turns into a silent bug the moment this function returns a value or
      // the `catch` rethrows.
      if (myGeneration === generation) {
        saving.value = false;
      }
    }
  }

  return { open, editingId, draft, saving, error, speakers, startEdit, cancelEdit, commit, reset };
}
