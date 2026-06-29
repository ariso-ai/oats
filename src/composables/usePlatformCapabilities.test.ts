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
});

describe('usePlatformCapabilities', () => {
  it('loads capabilities from the backend once', async () => {
    const caps = {
      os: 'windows',
      localBackend: { supported: false, engine: 'cpp-sidecar' },
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

  it('falls back to browser-derived defaults when the backend call fails', async () => {
    getPlatformCapabilities.mockRejectedValue(new Error('no backend'));
    await expect(loadPlatformCapabilities()).resolves.toEqual(defaultPlatformCapabilities());
  });
});
