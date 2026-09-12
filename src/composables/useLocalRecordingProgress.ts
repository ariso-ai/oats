import { ref, computed, onUnmounted, type Ref, type ComputedRef } from 'vue';
import { local, type RecordingStatusView } from '../tauri';

/** Stages of the local generation pipeline, surfaced to the detail panel. */
export type LocalProgressStage =
  | 'idle'
  | 'recording'
  | 'transcribing'
  | 'transcript-failed'
  | 'notes-pending'
  | 'notes-failed'
  /** Nothing was said: notes were skipped, and no retry can change that. */
  | 'notes-empty-transcript'
  | 'ready';

/** Derive the UI stage from a recording's status view. A present note ('ready')
 *  always wins; a `failed` recording status means transcription failed. */
export function deriveStage(s: RecordingStatusView | null): LocalProgressStage {
  if (!s) return 'idle';
  if (s.status === 'failed') return 'transcript-failed';
  // 'recording' means capture is still running (the stub meta.json written at
  // start, issue #355) — no generation has begun, and the recorder pill / strip
  // already communicate "recording", so the detail pane shows no status chip.
  // It is deliberately its own stage rather than 'idle': the poll loop has to
  // keep ticking through capture so it is still running when Stop flips the
  // status to 'transcribing'. Nothing restarts it if it stops here.
  if (s.status === 'recording') return 'recording';
  if (s.status === 'transcribing') return 'transcribing';
  // status === 'done'
  if (s.hasNote || s.notesStatus === 'ready') return 'ready';
  if (s.notesStatus === 'empty-transcript') return 'notes-empty-transcript';
  if (s.notesStatus === 'failed') return 'notes-failed';
  return 'notes-pending';
}

const POLL_MS = 2000;

/** Stages that are not terminal: capture is still running, or the generation
 *  pipeline is still working. Polling continues while the stage is one of these. */
const IN_FLIGHT_STAGES: readonly LocalProgressStage[] = ['recording', 'transcribing', 'notes-pending'];

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
 * capturing or generating (recording / transcribing / notes-pending) and stops
 * at any terminal stage (ready / failed). `getId` is read on each tick so the
 * caller can repoint it.
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
      shouldContinue = IN_FLIGHT_STAGES.includes(stage.value);
    } catch (e) {
      if (my !== token) return;
      console.error('local recording status poll failed', e);
      // Keep retrying if we don't have an initial snapshot yet, so a transient
      // error doesn't permanently hide in-flight generation progress.
      shouldContinue = status.value == null || IN_FLIGHT_STAGES.includes(stage.value);
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
    status.value = { status: 'transcribing', hasTranscript: false, hasNote: false, notesStatus: 'pending' };
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
    // Optimistic: show "Generating AI Notes" immediately. Poll only AFTER the
    // retry RPC resolves (it clears notes_error), so the first poll reflects the
    // regenerating state instead of the prior failure.
    status.value = { status: 'done', hasTranscript: true, hasNote: false, notesStatus: 'pending' };
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
