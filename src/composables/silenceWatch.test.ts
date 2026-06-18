import { describe, it, expect } from 'vitest';
import {
  shouldAutoStop,
  SILENCE_TIMEOUT_MS,
  shouldPromptSilence,
  shouldAutoStopAfterPrompt,
  SILENCE_PROMPT_MS,
  SILENCE_GRACE_MS,
} from './silenceWatch';

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

describe('shouldPromptSilence', () => {
  it('is false before the 10-minute trigger', () => {
    expect(shouldPromptSilence(0, SILENCE_PROMPT_MS - 1, false)).toBe(false);
  });

  it('is true at or past the 10-minute trigger', () => {
    expect(shouldPromptSilence(0, SILENCE_PROMPT_MS, false)).toBe(true);
    expect(shouldPromptSilence(0, SILENCE_PROMPT_MS + 5_000, false)).toBe(true);
  });

  it('is frozen (false) while paused, even past the trigger', () => {
    expect(shouldPromptSilence(0, SILENCE_PROMPT_MS + 60_000, true)).toBe(false);
  });
});

describe('shouldAutoStopAfterPrompt', () => {
  const shown = 1_000_000;

  it('is false before the 60-second grace elapses', () => {
    expect(shouldAutoStopAfterPrompt(shown, 0, shown + SILENCE_GRACE_MS - 1, false)).toBe(false);
  });

  it('is true at or past the grace window', () => {
    expect(shouldAutoStopAfterPrompt(shown, 0, shown + SILENCE_GRACE_MS, false)).toBe(true);
  });

  it('is cancelled (false) when audio resumed after the prompt', () => {
    // lastSoundAt advanced past promptShownAt → sound came back.
    expect(shouldAutoStopAfterPrompt(shown, shown + 1, shown + SILENCE_GRACE_MS + 10, false)).toBe(false);
  });

  it('is frozen (false) while paused, even past the grace window', () => {
    expect(shouldAutoStopAfterPrompt(shown, 0, shown + SILENCE_GRACE_MS + 60_000, true)).toBe(false);
  });
});
