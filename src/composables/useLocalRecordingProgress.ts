import { ref, computed, onUnmounted, type Ref, type ComputedRef } from 'vue';
import { local, type RecordingStatusView } from '../tauri';

/** Stages of the local generation pipeline, surfaced to the detail panel. */
export type LocalProgressStage =
  | 'idle'
  | 'transcribing'
  | 'transcript-failed'
  | 'notes-pending'
  | 'notes-updating'
  | 'notes-failed'
  | 'notes-update-failed'
  | 'ready';

/** Derive the UI stage from a recording's status view. Existing note content
 *  stays readable while an update runs or fails, so those states remain
 *  distinct from first-note generation/failure. */
export function deriveStage(s: RecordingStatusView | null): LocalProgressStage {
  if (!s) return 'idle';
  if (s.status === 'failed') return 'transcript-failed';
  if (s.status === 'recording' || s.status === 'transcribing') return 'transcribing';
  // status === 'done'
  if (s.notesStatus === 'updating') return 'notes-updating';
  if (s.notesStatus === 'failed') return s.hasNote ? 'notes-update-failed' : 'notes-failed';
  if (s.hasNote) return 'ready';
  return 'notes-pending';
}

const POLL_MS = 2000;

export interface LocalRecordingProgress {
  status: Ref<RecordingStatusView | null>;
  stage: ComputedRef<LocalProgressStage>;
  hasTranscript: ComputedRef<boolean>;
  hasNote: ComputedRef<boolean>;
  retrying: Ref<boolean>;
  /** Start (or restart) polling for the current id. */
  begin: () => void;
  /** Stop polling and clear status (used when switching away / to non-local). */
  reset: () => void;
  /** Stop polling, keeping the last status. */
  stop: () => void;
  retryTranscription: () => Promise<void>;
  retryNotes: () => Promise<void>;
}

/**
 * Polls `local.recordingStatus(id)` every 2s while the recording is still
 * generating (transcribing or notes-pending) and stops at any terminal stage
 * (ready / failed). `getId` is read on each tick so the caller can repoint it.
 * Retries set an optimistic status, resume polling, then fire the binding.
 */
export function useLocalRecordingProgress(getId: () => string | null): LocalRecordingProgress {
  const status = ref<RecordingStatusView | null>(null);
  const retrying = ref(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let token = 0;

  const stage = computed(() => deriveStage(status.value));
  const hasTranscript = computed(() => !!status.value?.hasTranscript);
  const hasNote = computed(() => !!status.value?.hasNote);

  function clearTimer(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  async function loop(my: number): Promise<void> {
    const id = getId();
    if (!id || my !== token) return;
    let shouldContinue = false;
    try {
      const s = await local.recordingStatus(id);
      if (my !== token) return;
      status.value = s;
      shouldContinue =
        stage.value === 'transcribing' ||
        stage.value === 'notes-pending' ||
        stage.value === 'notes-updating';
    } catch (e) {
      if (my !== token) return;
      console.error('local recording status poll failed', e);
      // Keep retrying if we don't have an initial snapshot yet, so a transient
      // error doesn't permanently hide in-flight generation progress.
      shouldContinue =
        status.value == null ||
        stage.value === 'transcribing' ||
        stage.value === 'notes-pending' ||
        stage.value === 'notes-updating';
    }
    if (my !== token) return;
    // Keep polling only while there is still work in flight.
    if (shouldContinue) {
      timer = setTimeout(() => void loop(my), POLL_MS);
    }
  }

  function begin(): void {
    clearTimer();
    const my = ++token;
    void loop(my);
  }

  function reset(): void {
    token++;
    clearTimer();
    status.value = null;
  }

  function stop(): void {
    token++;
    clearTimer();
  }

  async function retryTranscription(): Promise<void> {
    const id = getId();
    if (!id || retrying.value) return;
    retrying.value = true;
    // Optimistic: show "Generating Transcript" immediately. Poll only AFTER the
    // retry RPC resolves — until then the backend still reports the prior
    // terminal state, and a poll would clobber the optimistic stage and stop.
    status.value = {
      status: 'transcribing',
      hasTranscript: false,
      hasNote: false,
      notesStatus: 'pending',
    };
    try {
      await local.retryTranscription(id);
    } catch (e) {
      console.error('retry transcription failed', e);
    } finally {
      retrying.value = false;
    }
    begin();
  }

  async function retryNotes(): Promise<void> {
    const id = getId();
    if (!id || retrying.value) return;
    retrying.value = true;
    // Preserve a readable prior note while the retry/update runs. Poll only
    // after the RPC resolves so a stale failed snapshot cannot overwrite this
    // optimistic state before the backend records the active job.
    const hasExistingNote = !!status.value?.hasNote;
    status.value = {
      status: 'done',
      hasTranscript: status.value?.hasTranscript ?? true,
      hasNote: hasExistingNote,
      notesStatus: hasExistingNote ? 'updating' : 'pending',
      notesWritten: status.value?.notesWritten,
    };
    try {
      await local.retryNotes(id);
    } catch (e) {
      console.error('retry notes failed', e);
    } finally {
      retrying.value = false;
    }
    begin();
  }

  onUnmounted(stop);

  return { status, stage, hasTranscript, hasNote, retrying, begin, reset, stop, retryTranscription, retryNotes };
}
