/** Which situation the card is shown in: a call started while we were idle
 *  (`start`), the next calendar meeting started mid-recording (`switch`), or a
 *  call started that Ari is joining / already in, so oats won't record (`ari`,
 *  an informational notice with no actions). */
export type PromptMode = 'start' | 'switch' | 'ari';

/** What happens if the countdown runs out with no answer, which also decides the
 *  primary button: `record` (auto-record on) or `dismiss` (auto-record off). */
export type PromptDefaultAction = 'record' | 'dismiss';

export interface PromptParams {
  seconds: number;
  title: string;
  subtitle: string;
  mode: PromptMode;
  defaultAction: PromptDefaultAction;
}

/** Both modes show the same heading; only Rust's countdown and the subtitle
 *  fallback differ (switch mode hides the line when there's no meeting title). */
const DEFAULT_TITLE = 'Meeting started';
const START_DEFAULT_SECONDS = 10;
const START_DEFAULT_SUBTITLE = 'oats can take notes for you.';
/** Cosmetic countdown for switch mode; mirrors MEETING_SWITCH_PROMPT_TIMEOUT_MS. */
const SWITCH_DEFAULT_SECONDS = 30;
/** Cosmetic countdown for the Ari notice; mirrors ARI_NOTICE_DURATION. */
const ARI_DEFAULT_SECONDS = 6;
const ARI_DEFAULT_SUBTITLE = "Ari is taking notes, so oats won't record.";

function parseMode(raw: string | null): PromptMode {
  return raw === 'switch' || raw === 'ari' ? raw : 'start';
}

export function parsePromptParams(search: string): PromptParams {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  const mode = parseMode(params.get('mode'));
  const rawSeconds = Number(params.get('seconds'));
  const defaultSeconds =
    mode === 'switch' ? SWITCH_DEFAULT_SECONDS : mode === 'ari' ? ARI_DEFAULT_SECONDS : START_DEFAULT_SECONDS;
  const seconds = Number.isFinite(rawSeconds) && rawSeconds > 0 ? rawSeconds : defaultSeconds;
  if (mode === 'ari') {
    const joining = params.get('ari') === 'joining';
    return {
      seconds,
      title: joining ? 'Ari is joining this meeting' : 'Ari has joined this meeting',
      subtitle: params.get('subtitle') || ARI_DEFAULT_SUBTITLE,
      mode,
      // The notice only ever times out into a no-op.
      defaultAction: 'dismiss',
    };
  }
  return {
    seconds,
    title: params.get('title') || DEFAULT_TITLE,
    subtitle: params.get('subtitle') || (mode === 'switch' ? '' : START_DEFAULT_SUBTITLE),
    mode,
    defaultAction: params.get('default') === 'dismiss' ? 'dismiss' : 'record',
  };
}

export interface SilencePromptParams {
  seconds: number;
  /** Meeting title, or '' when there's no associated meeting (subtitle hidden). */
  subtitle: string;
}

/** Grace window before a silent recording auto-stops; drives the countdown bar. */
const SILENCE_DEFAULT_SECONDS = 60;

/**
 * Params for the silence-stop prompt window. Unlike the meeting prompt, the
 * subtitle is left empty when absent (the view hides the line) rather than
 * falling back to a default, and the title is fixed by the view.
 */
export function parseSilencePromptParams(search: string): SilencePromptParams {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  const rawSeconds = Number(params.get('seconds'));
  const seconds =
    Number.isFinite(rawSeconds) && rawSeconds > 0 ? rawSeconds : SILENCE_DEFAULT_SECONDS;
  return { seconds, subtitle: params.get('subtitle') || '' };
}
