import { describe, it, expect, vi, beforeEach } from 'vitest';

const getPlatformCapabilities = vi.hoisted(() => vi.fn());

vi.mock('../tauri', () => ({
  getPlatformCapabilities: () => getPlatformCapabilities(),
}));

import {
  defaultPlatformCapabilities,
  loadPlatformCapabilities,
  resetPlatformCapabilitiesCache,
} from './usePlatformCapabilities';

beforeEach(() => {
  vi.clearAllMocks();
  resetPlatformCapabilitiesCache();
  Object.defineProperty(globalThis, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {},
  });
});

describe('usePlatformCapabilities', () => {
  it('loads capabilities from the backend once', async () => {
    const caps = {
      os: 'windows',
      localBackend: { supported: true, engine: 'cpp-sidecar' },
      systemAudio: { supported: false, settingsUrl: 'ms-settings:sound' },
      autoRecord: { supported: false },
      nativeShare: { supported: false },
      notificationSettingsUrl: 'ms-settings:notifications',
      microphoneSettingsUrl: 'ms-settings:privacy-microphone',
    };
    getPlatformCapabilities.mockResolvedValue(caps);

    await expect(loadPlatformCapabilities()).resolves.toBe(caps);
    await expect(loadPlatformCapabilities()).resolves.toBe(caps);
    expect(getPlatformCapabilities).toHaveBeenCalledTimes(1);
  });

  it('surfaces backend failures inside Tauri', async () => {
    getPlatformCapabilities.mockRejectedValue(new Error('no backend'));
    await expect(loadPlatformCapabilities()).rejects.toThrow('no backend');
  });

  it('uses unsupported defaults only outside Tauri', async () => {
    delete (globalThis as typeof globalThis & { __TAURI_INTERNALS__?: unknown })
      .__TAURI_INTERNALS__;
    await expect(loadPlatformCapabilities()).resolves.toEqual(defaultPlatformCapabilities());
    expect(getPlatformCapabilities).not.toHaveBeenCalled();
  });
});
