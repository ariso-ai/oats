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
 * Status text shown for a model that is NOT yet installed. (An installed model
 * is shown with a green tick in the template instead of any text.) A live
 * download/error takes precedence; otherwise the model is simply not downloaded.
 */
export function rowStatusText(busy: Busy, progress: number | null): string {
  if (busy === 'downloading') {
    return progress == null ? 'Downloading…' : `Downloading ${Math.round(progress * 100)}%`;
  }
  if (busy === 'error') return 'Download failed';
  return 'Not downloaded';
}
