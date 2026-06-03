import { describe, it, expect } from 'vitest';
import { shouldAutoDownload } from './settingsDownload';

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
