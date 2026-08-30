import { describe, it, expect } from 'vitest';
import { sanitizeFilename, isoDateStamp, transcriptFilename } from './transcriptDownloadName';

describe('sanitizeFilename', () => {
  it('keeps an ordinary title unchanged', () => {
    expect(sanitizeFilename('Weekly sync')).toBe('Weekly sync');
  });

  it('replaces path-illegal characters and collapses the gaps', () => {
    expect(sanitizeFilename('Q3 / Q4: plan?')).toBe('Q3 Q4 plan');
  });

  it('falls back to Meeting for a blank title', () => {
    expect(sanitizeFilename('')).toBe('Meeting');
    expect(sanitizeFilename(undefined)).toBe('Meeting');
  });

  it('falls back to Meeting when the title sanitizes away to nothing', () => {
    expect(sanitizeFilename('///')).toBe('Meeting');
  });
});

describe('isoDateStamp', () => {
  it('formats a timestamp as YYYY-MM-DD', () => {
    // Midday UTC so the local-time conversion cannot cross a date boundary
    // in any timezone CI might run in.
    expect(isoDateStamp('2026-08-30T12:00:00Z')).toBe('2026-08-30');
  });

  it('returns empty string for a missing or unparseable timestamp', () => {
    expect(isoDateStamp(undefined)).toBe('');
    expect(isoDateStamp('not a date')).toBe('');
  });
});

describe('transcriptFilename', () => {
  it('assembles title, suffix, and date', () => {
    expect(transcriptFilename('Weekly sync', '2026-08-30T12:00:00Z')).toBe(
      'Weekly sync transcript - 2026-08-30.md'
    );
  });

  it('omits the date separator when the date is unknown', () => {
    expect(transcriptFilename('Weekly sync', undefined)).toBe('Weekly sync transcript.md');
  });

  it('applies the default title fallback', () => {
    expect(transcriptFilename('', '2026-08-30T12:00:00Z')).toBe(
      'Meeting transcript - 2026-08-30.md'
    );
  });
});
