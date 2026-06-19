import { describe, it, expect } from 'vitest';
import { shouldPromptDownload, rowStatusText, pendingInstalls, modelBannerVisible } from './settingsDownload';

describe('shouldPromptDownload', () => {
  it('prompts for local on first switch when models are missing', () => {
    expect(shouldPromptDownload('local', false, 'not_downloaded')).toBe(true);
    expect(shouldPromptDownload('local', false, 'error')).toBe(true);
  });
  it('does not prompt once the user has already been prompted', () => {
    expect(shouldPromptDownload('local', true, 'not_downloaded')).toBe(false);
  });
  it('does not prompt when ready, downloading, or unsupported', () => {
    expect(shouldPromptDownload('local', false, 'ready')).toBe(false);
    expect(shouldPromptDownload('local', false, 'downloading')).toBe(false);
    expect(shouldPromptDownload('local', false, 'unsupported')).toBe(false);
  });
  it('never prompts for the ariso backend', () => {
    expect(shouldPromptDownload('ariso', false, 'not_downloaded')).toBe(false);
  });
});

describe('rowStatusText', () => {
  it('shows a bare percentage while downloading', () => {
    expect(rowStatusText('downloading', 0.42)).toBe('42%');
    expect(rowStatusText('downloading', 0.9)).toBe('90%');
    expect(rowStatusText('downloading', null)).toBe('Starting…');
  });
  it('shows failure on error', () => {
    expect(rowStatusText('error', null)).toBe('Download failed');
  });
  it('shows "Not downloaded" when idle and not installed', () => {
    expect(rowStatusText('idle', null)).toBe('Not downloaded');
  });
});

describe('pendingInstalls', () => {
  it('flags both models when neither is ready and neither is downloading', () => {
    expect(
      pendingInstalls({ state: 'not_downloaded', llmReady: false }, 'idle', 'idle'),
    ).toEqual({ stt: true, llm: true });
  });

  it('skips a model that is already ready', () => {
    expect(
      pendingInstalls({ state: 'ready', llmReady: false }, 'idle', 'idle'),
    ).toEqual({ stt: false, llm: true });
  });

  it('skips a model that is already downloading', () => {
    expect(
      pendingInstalls({ state: 'not_downloaded', llmReady: false }, 'downloading', 'downloading'),
    ).toEqual({ stt: false, llm: false });
  });

  it('installs neither model on an unsupported platform', () => {
    expect(
      pendingInstalls({ state: 'unsupported', llmReady: false }, 'idle', 'idle'),
    ).toEqual({ stt: false, llm: false });
  });

  it('does not flag STT while the backend reports a download in progress', () => {
    expect(
      pendingInstalls({ state: 'downloading', llmReady: false }, 'idle', 'idle').stt,
    ).toBe(false);
  });
});

describe('modelBannerVisible', () => {
  it('is hidden when not prompted', () => {
    expect(modelBannerVisible(false, false, false)).toBe(false);
  });

  it('is shown while either model is incomplete', () => {
    expect(modelBannerVisible(true, false, true)).toBe(true);
    expect(modelBannerVisible(true, true, false)).toBe(true);
  });

  it('is hidden once both models are installed', () => {
    expect(modelBannerVisible(true, true, true)).toBe(false);
  });
});
