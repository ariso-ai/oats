import { describe, it, expect, vi, beforeEach } from 'vitest';

const storeGet = vi.fn();
const storeSet = vi.fn(() => Promise.resolve());

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({ get: storeGet, set: storeSet }),
}));

import {
  isSilenceDetectionEnabled,
  setSilenceDetectionEnabled,
} from './useSilenceDetection';

beforeEach(() => vi.clearAllMocks());

describe('useSilenceDetection', () => {
  it('defaults to enabled when unset', async () => {
    storeGet.mockResolvedValue(undefined);
    expect(await isSilenceDetectionEnabled()).toBe(true);
  });

  it('stays enabled when explicitly true', async () => {
    storeGet.mockResolvedValue(true);
    expect(await isSilenceDetectionEnabled()).toBe(true);
  });

  it('is disabled only when explicitly false', async () => {
    storeGet.mockResolvedValue(false);
    expect(await isSilenceDetectionEnabled()).toBe(false);
  });

  it('persists the flag on set', async () => {
    await setSilenceDetectionEnabled(false);
    expect(storeSet).toHaveBeenCalledWith('silenceDetectionEnabled', false);
  });
});
