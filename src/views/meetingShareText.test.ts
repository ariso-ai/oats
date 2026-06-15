import { describe, it, expect } from 'vitest';
import { composeLocalShareText } from './meetingShareText';

const base = { title: 'Standup', startAt: '2026-06-01T10:00:00Z', note: 'AI body' };

describe('composeLocalShareText', () => {
  it('includes both notes under a title heading', () => {
    const out = composeLocalShareText(base, 'my thoughts');
    expect(out).toContain('# Standup');
    expect(out).toContain('## AI Notes');
    expect(out).toContain('AI body');
    expect(out).toContain('## My Notes');
    expect(out).toContain('my thoughts');
  });

  it('omits the AI section when there is no AI note', () => {
    const out = composeLocalShareText({ ...base, note: '' }, 'only mine');
    expect(out).not.toContain('## AI Notes');
    expect(out).toContain('## My Notes');
  });

  it('omits the My Notes section when the personal note is blank', () => {
    const out = composeLocalShareText(base, '   ');
    expect(out).toContain('## AI Notes');
    expect(out).not.toContain('## My Notes');
  });

  it('returns empty string when there is nothing to share', () => {
    expect(composeLocalShareText({ ...base, note: '' }, '')).toBe('');
  });

  it('falls back to a default title and skips an invalid date', () => {
    const out = composeLocalShareText({ title: '', startAt: 'not-a-date', note: 'x' }, '');
    expect(out).toContain('# Meeting notes');
    expect(out).not.toContain('not-a-date');
  });
});
