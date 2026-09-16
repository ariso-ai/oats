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
    try {
      const markdown = await local.renameSpeaker(id, speakerId, label);
      // Patched from the command's own result rather than re-read: the label we
      // sent and the transcript it rendered are the authoritative pair.
      deps.speakers.value = deps.speakers.value.map((p) =>
        p.id === speakerId ? { ...p, label } : p
      );
      deps.transcript.value = markdown;
      editingId.value = null;
      draft.value = '';
    } catch (e) {
      // A Tauri command rejects with a bare string, so this keeps the backend's
      // own wording (e.g. the "predates structured transcripts" case).
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      saving.value = false;
    }
  }

  return { open, editingId, draft, saving, error, speakers, startEdit, cancelEdit, commit, reset };
}
