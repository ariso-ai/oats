import { describe, it, expect, vi, beforeEach } from 'vitest';

const storeGet = vi.fn();
const storeSet = vi.fn(() => Promise.resolve());
const emit = vi.fn(() => Promise.resolve());
const invoke = vi.fn();

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({ get: storeGet, set: storeSet }),
}));
vi.mock('@tauri-apps/api/event', () => ({ emit: (...a: unknown[]) => emit(...a) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const platformSupported = vi.hoisted(() => vi.fn(() => true));
vi.mock('./usePlatformCapabilities', () => ({
  loadPlatformCapabilities: () =>
    Promise.resolve({ autoRecord: { supported: platformSupported() } }),
}));

import {
  isAutoRecordEnabled,
  setAutoRecordEnabled,
  isAutoRecordSupported,
  AUTO_RECORD_SYNC_EVENT,
} from './useAutoRecord';

beforeEach(() => {
  vi.clearAllMocks();
  platformSupported.mockReturnValue(true);
});

describe('useAutoRecord', () => {
  it('defaults to enabled when unset', async () => {
    storeGet.mockResolvedValue(undefined);
    expect(await isAutoRecordEnabled()).toBe(true);
  });

  it('is disabled only when explicitly false', async () => {
    storeGet.mockResolvedValue(false);
    expect(await isAutoRecordEnabled()).toBe(false);
  });

  it('persists and broadcasts a sync on set', async () => {
    await setAutoRecordEnabled(false);
    expect(storeSet).toHaveBeenCalledWith('autoRecordEnabled', false);
    expect(emit).toHaveBeenCalledWith(AUTO_RECORD_SYNC_EVENT);
  });

  it('reports support via the native command, false on error', async () => {
    invoke.mockResolvedValueOnce(true);
    expect(await isAutoRecordSupported()).toBe(true);
    invoke.mockRejectedValueOnce(new Error('nope'));
    expect(await isAutoRecordSupported()).toBe(false);
  });

  it('does not call the native probe when the platform says auto-record is unsupported', async () => {
    platformSupported.mockReturnValue(false);
    expect(await isAutoRecordSupported()).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });
});
