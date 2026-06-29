export interface PromptParams {
  seconds: number;
  title: string;
  subtitle: string;
}

const DEFAULTS: PromptParams = {
  seconds: 10,
  title: 'Meeting started',
  subtitle: 'oats can take notes for you.',
};

export function parsePromptParams(search: string): PromptParams {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  const rawSeconds = Number(params.get('seconds'));
  const seconds = Number.isFinite(rawSeconds) && rawSeconds > 0 ? rawSeconds : DEFAULTS.seconds;
  return {
    seconds,
    title: params.get('title') || DEFAULTS.title,
    subtitle: params.get('subtitle') || DEFAULTS.subtitle,
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

/** Cosmetic countdown for the meeting-end prompt; mirrors the FE timeout. */
const MEETING_END_DEFAULT_SECONDS = 30;

/**
 * Params for the meeting-end stop prompt window. Same shape as the silence
 * prompt (fixed title in the view; subtitle blank when absent), but a 30s
 * default countdown.
 */
export function parseMeetingEndPromptParams(search: string): SilencePromptParams {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  const rawSeconds = Number(params.get('seconds'));
  const seconds =
    Number.isFinite(rawSeconds) && rawSeconds > 0 ? rawSeconds : MEETING_END_DEFAULT_SECONDS;
  return { seconds, subtitle: params.get('subtitle') || '' };
}
