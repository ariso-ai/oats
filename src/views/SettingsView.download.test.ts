import { describe, it, expect } from 'vitest';
import { shouldAutoDownload, llmRowState } from './settingsDownload';

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

describe('llmRowState', () => {
  it('reflects gemma presence when not actively downloading', () => {
    expect(llmRowState('ready', true)).toBe('ready');
    expect(llmRowState('ready', false)).toBe('not_downloaded');
    expect(llmRowState('not_downloaded', undefined)).toBe('not_downloaded');
  });
  it('shares the overall state while downloading or errored', () => {
    expect(llmRowState('downloading', false)).toBe('downloading');
    expect(llmRowState('error', false)).toBe('error');
    expect(llmRowState('unsupported', undefined)).toBe('unsupported');
  });
});
