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
    expect(rowStatusText(false, 'downloading', 0.42)).toBe('Downloading 42%');
    expect(rowStatusText(false, 'downloading', null)).toBe('Downloading…');
  });
  it('shows failure on error', () => {
    expect(rowStatusText(false, 'error', null)).toBe('Download failed');
  });
  it('reflects installed state when idle', () => {
    expect(rowStatusText(true, 'idle', null)).toBe('Ready');
    expect(rowStatusText(false, 'idle', null)).toBe('Not downloaded');
  });
  it('uses the provided ready label when installed', () => {
    expect(rowStatusText(true, 'idle', null, 'Ready (parakeet-tdt-0.6b-v3)')).toBe(
      'Ready (parakeet-tdt-0.6b-v3)',
    );
  });
});
