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
