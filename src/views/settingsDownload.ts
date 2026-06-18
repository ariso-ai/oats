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

/**
 * Which on-device models still need a download kicked off. A model is "pending"
 * when it is not already installed and not already mid-download. STT is never
 * pending on an unsupported platform (its download would fail). Used by the
 * recording gate's `tray://show-model-prompt` handler to auto-start downloads.
 */
export function pendingInstalls(
  status: ModelStatus,
  sttBusy: Busy,
  llmBusy: Busy,
): { stt: boolean; llm: boolean } {
  const stt =
    status.state !== 'ready' &&
    status.state !== 'unsupported' &&
    status.state !== 'downloading' &&
    sttBusy !== 'downloading';
  const llm =
    status.state !== 'unsupported' &&
    status.llmReady !== true &&
    llmBusy !== 'downloading';
  return { stt, llm };
}

/**
 * Whether to show the "download the models to record" banner: only after the
 * recording gate has prompted, and only while at least one model is incomplete.
 * A model is "satisfied" when it is installed (or, for STT, can never install on
 * an unsupported platform) — so the banner does not linger on such hardware.
 */
export function modelBannerVisible(
  prompted: boolean,
  sttSatisfied: boolean,
  llmSatisfied: boolean,
): boolean {
  return prompted && (!sttSatisfied || !llmSatisfied);
}
