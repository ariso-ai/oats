import type { ModelStatus } from '../tauri';

/** Whether switching to `backend` with the given model state should auto-start a download. */
export function shouldAutoDownload(
  backend: 'ariso' | 'local',
  state: ModelStatus['state'],
): boolean {
  if (backend !== 'local') return false;
  return state !== 'ready' && state !== 'downloading' && state !== 'unsupported';
}
