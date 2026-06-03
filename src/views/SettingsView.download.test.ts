import { describe, it, expect } from 'vitest';
import { shouldAutoDownload, rowStatusText } from './settingsDownload';

describe('shouldAutoDownload', () => {
  it('starts for local when not yet downloaded', () => {
    expect(shouldAutoDownload('local', 'not_downloaded')).toBe(true);
    expect(shouldAutoDownload('local', 'error')).toBe(true);
  });
  it('does not start when ready, downloading, or unsupported', () => {
    expect(shouldAutoDownload('local', 'ready')).toBe(false);
    expect(shouldAutoDownload('local', 'downloading')).toBe(false);
    expect(shouldAutoDownload('local', 'unsupported')).toBe(false);
  });
  it('never starts for the ariso backend', () => {
    expect(shouldAutoDownload('ariso', 'not_downloaded')).toBe(false);
  });
});

describe('rowStatusText', () => {
  it('shows a download percentage while downloading', () => {
    expect(rowStatusText('downloading', 0.42)).toBe('Downloading 42%');
    expect(rowStatusText('downloading', null)).toBe('Downloading…');
  });
  it('shows failure on error', () => {
    expect(rowStatusText('error', null)).toBe('Download failed');
  });
  it('shows "Not downloaded" when idle and not installed', () => {
    expect(rowStatusText('idle', null)).toBe('Not downloaded');
  });
});
