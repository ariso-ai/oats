import type { ModelStatus } from '../tauri';

/** Per-model install/download UI state, independent of the backend status. */
export type Busy = 'idle' | 'downloading' | 'error';

/** Whether switching to `backend` with the given STT state should auto-start the STT download. */
export function shouldAutoDownload(
  backend: 'ariso' | 'local',
  state: ModelStatus['state'],
): boolean {
  if (backend !== 'local') return false;
  return state !== 'ready' && state !== 'downloading' && state !== 'unsupported';
}

/**
 * Status text for a single model row. A live download/error (`busy`) takes
 * precedence; otherwise the row reflects whether the model is installed.
 * `readyLabel` lets the STT row show its version (e.g. "Ready (parakeet…)").
 */
export function rowStatusText(
  installed: boolean,
  busy: Busy,
  progress: number | null,
  readyLabel = 'Ready',
): string {
  if (busy === 'downloading') {
    return progress == null ? 'Downloading…' : `Downloading ${Math.round(progress * 100)}%`;
  }
  if (busy === 'error') return 'Download failed';
  return installed ? readyLabel : 'Not downloaded';
}
