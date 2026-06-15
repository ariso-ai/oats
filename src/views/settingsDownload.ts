import type { ModelStatus } from '../tauri';

/** Per-model install/download UI state, independent of the backend status. */
export type Busy = 'idle' | 'downloading' | 'error';

/**
 * Whether switching to `backend` should open the first-time "download models"
 * confirm dialog. Only for Local, only when the user has not been prompted
 * before, and only when the models are not already ready / unsupported / mid-download.
 */
export function shouldPromptDownload(
  backend: 'ariso' | 'local',
  alreadyPrompted: boolean,
  state: ModelStatus['state'],
): boolean {
  if (backend !== 'local' || alreadyPrompted) return false;
  return state !== 'ready' && state !== 'downloading' && state !== 'unsupported';
}

/**
 * Status text shown beside a model that is NOT yet installed. (An installed
 * model is shown with a green tick in the template instead of any text.) While
 * downloading, this is the bare progress percentage (e.g. "90%") — the button
 * itself carries the "Downloading" label.
 */
export function rowStatusText(busy: Busy, progress: number | null): string {
  if (busy === 'downloading') {
    return progress == null ? 'Starting…' : `${Math.round(progress * 100)}%`;
  }
  if (busy === 'error') return 'Download failed';
  return 'Not downloaded';
}
