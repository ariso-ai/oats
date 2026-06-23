import { describe, it, expect } from 'vitest';
import { parsePromptParams, parseSilencePromptParams } from './meetingPromptParams';

describe('parsePromptParams', () => {
  it('uses defaults when the query is empty', () => {
    expect(parsePromptParams('')).toEqual({
      seconds: 10,
      title: 'Meeting started',
      subtitle: 'oats can take notes for you.',
    });
  });

  it('reads seconds from the query (with or without leading ?)', () => {
    expect(parsePromptParams('?seconds=7').seconds).toBe(7);
    expect(parsePromptParams('seconds=7').seconds).toBe(7);
  });

  it('falls back to 10 when seconds is missing, zero, negative, or non-numeric', () => {
    expect(parsePromptParams('?seconds=0').seconds).toBe(10);
    expect(parsePromptParams('?seconds=-3').seconds).toBe(10);
    expect(parsePromptParams('?seconds=abc').seconds).toBe(10);
    expect(parsePromptParams('?foo=bar').seconds).toBe(10);
  });

  it('overrides title and subtitle when present', () => {
    const p = parsePromptParams('?title=Standup&subtitle=go');
    expect(p.title).toBe('Standup');
    expect(p.subtitle).toBe('go');
  });
});

describe('parseSilencePromptParams', () => {
  it('defaults to 60s and an empty subtitle when the query is empty', () => {
    expect(parseSilencePromptParams('')).toEqual({ seconds: 60, subtitle: '' });
  });

  it('reads seconds (with or without leading ?), falling back to 60 when invalid', () => {
    expect(parseSilencePromptParams('?seconds=45').seconds).toBe(45);
    expect(parseSilencePromptParams('seconds=45').seconds).toBe(45);
    expect(parseSilencePromptParams('?seconds=0').seconds).toBe(60);
    expect(parseSilencePromptParams('?seconds=-3').seconds).toBe(60);
    expect(parseSilencePromptParams('?seconds=abc').seconds).toBe(60);
  });

  it('reads the subtitle but leaves it empty when absent (so the view hides it)', () => {
    expect(parseSilencePromptParams('?subtitle=Weekly%20sync').subtitle).toBe('Weekly sync');
    expect(parseSilencePromptParams('?seconds=60').subtitle).toBe('');
  });
});
