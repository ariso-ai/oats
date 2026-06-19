import { describe, it, expect } from 'vitest';
import { shouldAutoStop, SILENCE_TIMEOUT_MS } from './silenceWatch';

describe('shouldAutoStop', () => {
  it('is false before the timeout elapses', () => {
    expect(shouldAutoStop(0, SILENCE_TIMEOUT_MS - 1, false)).toBe(false);
  });

  it('is true at or past the timeout', () => {
    expect(shouldAutoStop(0, SILENCE_TIMEOUT_MS, false)).toBe(true);
    expect(shouldAutoStop(0, SILENCE_TIMEOUT_MS + 5_000, false)).toBe(true);
  });

  it('is frozen (false) while paused, even past the timeout', () => {
    expect(shouldAutoStop(0, SILENCE_TIMEOUT_MS + 60_000, true)).toBe(false);
  });

  it('honors a custom timeout', () => {
    expect(shouldAutoStop(1_000, 2_000, false, 2_000)).toBe(false);
    expect(shouldAutoStop(1_000, 3_000, false, 2_000)).toBe(true);
  });
});
