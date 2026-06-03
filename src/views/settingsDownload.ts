import type { ModelStatus } from '../tauri';

/** Whether switching to `backend` with the given model state should auto-start a download. */
export function shouldAutoDownload(
  backend: 'ariso' | 'local',
  state: ModelStatus['state'],
): boolean {
  if (backend !== 'local') return false;
  return state !== 'ready' && state !== 'downloading' && state !== 'unsupported';
}

/**
 * Display state for the notes-LLM (gemma) status row. While the overall model
 * download is in progress (or errored/unsupported), the LLM shares that state;
 * otherwise it reflects whether the gemma model is actually present on disk.
 */
export function llmRowState(
  overall: ModelStatus['state'],
  llmReady: boolean | undefined,
): ModelStatus['state'] {
  if (overall === 'downloading' || overall === 'error' || overall === 'unsupported') {
    return overall;
  }
  return llmReady ? 'ready' : 'not_downloaded';
}
