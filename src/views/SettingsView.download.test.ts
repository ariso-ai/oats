import { describe, it, expect } from 'vitest';
import { shouldAutoDownload, llmRowState, isModelInstalled } from './settingsDownload';

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

describe('isModelInstalled', () => {
  it('is true only when manifest ready AND the LLM is present', () => {
    expect(isModelInstalled('ready', true)).toBe(true);
  });
  it('is false when the LLM is missing even if the manifest is ready', () => {
    expect(isModelInstalled('ready', false)).toBe(false);
    expect(isModelInstalled('ready', undefined)).toBe(false);
  });
  it('is false for non-ready states', () => {
    expect(isModelInstalled('not_downloaded', false)).toBe(false);
    expect(isModelInstalled('downloading', false)).toBe(false);
    expect(isModelInstalled('error', false)).toBe(false);
    expect(isModelInstalled('unsupported', false)).toBe(false);
  });
});
