import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  values: new Map<string, unknown>(),
  set: vi.fn(),
  loadPlatformCapabilities: vi.fn(),
  requestMicrophonePermission: vi.fn(),
  checkMicrophonePermission: vi.fn(),
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
vi.mock('../tauri', () => ({
  requestMicrophonePermission: () => mocks.requestMicrophonePermission(),
  checkMicrophonePermission: () => mocks.checkMicrophonePermission(),
}));

import {
  checkMicPermission,
  ensureMicPermission,
  loadRecordingEnabled,
} from './useRecordingPermissions';

beforeEach(() => {
  mocks.values.clear();
  mocks.set.mockReset();
  mocks.set.mockImplementation(async (key: string, value: unknown) => {
    mocks.values.set(key, value);
  });
  mocks.loadPlatformCapabilities.mockReset();
  mocks.requestMicrophonePermission.mockReset();
  mocks.checkMicrophonePermission.mockReset();
});

describe('microphone permissions', () => {
  it('uses native permission commands on macOS', async () => {
    mocks.loadPlatformCapabilities.mockResolvedValue({ os: 'macos' });
    mocks.requestMicrophonePermission.mockResolvedValue(true);
    mocks.checkMicrophonePermission.mockResolvedValue(true);

    await expect(ensureMicPermission()).resolves.toBe(true);
    await expect(checkMicPermission()).resolves.toBe(true);
    expect(mocks.requestMicrophonePermission).toHaveBeenCalledOnce();
    expect(mocks.checkMicrophonePermission).toHaveBeenCalledOnce();
  });

  it('uses browser-owned permission APIs on Windows', async () => {
    mocks.loadPlatformCapabilities.mockResolvedValue({ os: 'windows' });
    const stop = vi.fn();
    const getUserMedia = vi.fn(async () => ({ getTracks: () => [{ stop }] }));
    const query = vi.fn(async () => ({ state: 'granted' }));
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: { getUserMedia },
    });
    Object.defineProperty(navigator, 'permissions', {
      configurable: true,
      value: { query },
    });

    await expect(ensureMicPermission()).resolves.toBe(true);
    await expect(checkMicPermission()).resolves.toBe(true);
    expect(getUserMedia).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
    expect(query).toHaveBeenCalledWith({ name: 'microphone' });
    expect(mocks.requestMicrophonePermission).not.toHaveBeenCalled();
  });
});

describe('loadRecordingEnabled', () => {
  it('clears an existing system-only choice when system audio is unsupported', async () => {
    mocks.values.set('recordMicEnabled', false);
    mocks.values.set('recordSystemAudioEnabled', true);
    mocks.loadPlatformCapabilities.mockResolvedValue({
      systemAudio: { supported: false },
    });

    await expect(loadRecordingEnabled()).resolves.toEqual({
      mic: false,
      systemAudio: false,
    });
    expect(mocks.set).toHaveBeenCalledOnce();
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

  it('uses a capability-aware default for a fresh install', async () => {
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
});
