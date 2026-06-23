import { describe, it, expect } from 'vitest';
import { parsePromptParams } from './meetingPromptParams';

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
