import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  values: new Map<string, unknown>(),
  set: vi.fn(),
  loadPlatformCapabilities: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({
    get: (key: string) => Promise.resolve(mocks.values.get(key)),
    set: (key: string, value: unknown) => mocks.set(key, value),
  }),
}));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('./usePlatformCapabilities', () => ({
  loadPlatformCapabilities: () => mocks.loadPlatformCapabilities(),
}));

import { loadRecordingEnabled } from './useRecordingPermissions';

beforeEach(() => {
  mocks.values.clear();
  mocks.set.mockReset();
  mocks.set.mockImplementation(async (key: string, value: unknown) => {
    mocks.values.set(key, value);
  });
  mocks.loadPlatformCapabilities.mockReset();
});

describe('loadRecordingEnabled', () => {
  it('persists a usable microphone-only mode when system audio is unsupported', async () => {
    mocks.values.set('recordMicEnabled', false);
    mocks.values.set('recordSystemAudioEnabled', true);
    mocks.loadPlatformCapabilities.mockResolvedValue({
      systemAudio: { supported: false },
    });

    await expect(loadRecordingEnabled()).resolves.toEqual({
      mic: true,
      systemAudio: false,
    });
    expect(mocks.set).toHaveBeenCalledWith('recordMicEnabled', true);
    expect(mocks.set).toHaveBeenCalledWith('recordSystemAudioEnabled', false);
  });

  it('preserves a supported system-only preference', async () => {
    mocks.values.set('recordMicEnabled', false);
    mocks.values.set('recordSystemAudioEnabled', true);
    mocks.loadPlatformCapabilities.mockResolvedValue({
      systemAudio: { supported: true },
    });

    await expect(loadRecordingEnabled()).resolves.toEqual({
      mic: false,
      systemAudio: true,
    });
    expect(mocks.set).not.toHaveBeenCalled();
  });
});
