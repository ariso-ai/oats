import { describe, it, expect } from 'vitest';
import { shouldPromptDownload, rowStatusText } from './settingsDownload';

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
