import { describe, it, expect, vi } from 'vitest';
import {
  deriveEnabledFromLegacy,
  deriveRecordingMode,
  permissionStatus,
  applyToggle,
} from './recordingSettings';

describe('deriveEnabledFromLegacy', () => {
  it('maps legacy "mic" to mic-only', () => {
    expect(deriveEnabledFromLegacy('mic')).toEqual({ mic: true, systemAudio: false });
  });
  it('maps legacy "mic_and_system" to both on', () => {
    expect(deriveEnabledFromLegacy('mic_and_system')).toEqual({ mic: true, systemAudio: true });
  });
  it('defaults to both on when absent or unknown', () => {
    expect(deriveEnabledFromLegacy(undefined)).toEqual({ mic: true, systemAudio: true });
    expect(deriveEnabledFromLegacy(null)).toEqual({ mic: true, systemAudio: true });
    expect(deriveEnabledFromLegacy('garbage')).toEqual({ mic: true, systemAudio: true });
  });
});

describe('deriveRecordingMode', () => {
  it('returns the correct mode for each combination', () => {
    expect(deriveRecordingMode({ mic: true, systemAudio: true })).toBe('mic_and_system');
    expect(deriveRecordingMode({ mic: true, systemAudio: false })).toBe('mic');
    expect(deriveRecordingMode({ mic: false, systemAudio: true })).toBe('system');
    expect(deriveRecordingMode({ mic: false, systemAudio: false })).toBeNull();
  });
});

describe('permissionStatus', () => {
  it('maps a boolean to a status string', () => {
    expect(permissionStatus(true)).toBe('granted');
    expect(permissionStatus(false)).toBe('denied');
  });
});

describe('applyToggle', () => {
  const makeDeps = (over = {}) => ({
    ensurePermission: vi.fn().mockResolvedValue(true),
    openSettings: vi.fn().mockResolvedValue(undefined),
    persist: vi.fn().mockResolvedValue(undefined),
    ...over,
  });

  it('turning on with permission granted persists and reports granted', async () => {
    const deps = makeDeps();
    const res = await applyToggle(true, false, deps);
    expect(res).toEqual({ enabled: true, status: 'granted' });
    expect(deps.openSettings).not.toHaveBeenCalled();
    expect(deps.persist).toHaveBeenCalledWith(true);
  });

  it('turning on when denied opens settings and still persists', async () => {
    const deps = makeDeps({ ensurePermission: vi.fn().mockResolvedValue(false) });
    const res = await applyToggle(true, false, deps);
    expect(res).toEqual({ enabled: true, status: 'denied' });
    expect(deps.openSettings).toHaveBeenCalledOnce();
    expect(deps.persist).toHaveBeenCalledWith(true);
  });

  it('treats a thrown permission request as denied but still persists', async () => {
    const deps = makeDeps({ ensurePermission: vi.fn().mockRejectedValue(new Error('boom')) });
    const res = await applyToggle(true, false, deps);
    expect(res.status).toBe('denied');
    expect(deps.persist).toHaveBeenCalledWith(true);
  });

  it('reverts to the previous value when persisting fails', async () => {
    const deps = makeDeps({ persist: vi.fn().mockRejectedValue(new Error('disk full')) });
    const res = await applyToggle(true, false, deps);
    expect(res).toEqual({ enabled: false, status: 'granted' });
  });

  it('turning off clears status, persists false, and never requests permission', async () => {
    const deps = makeDeps();
    const res = await applyToggle(false, true, deps);
    expect(res).toEqual({ enabled: false, status: '' });
    expect(deps.ensurePermission).not.toHaveBeenCalled();
    expect(deps.persist).toHaveBeenCalledWith(false);
  });

  it('keeps denied status and still persists when openSettings throws', async () => {
    const deps = makeDeps({
      ensurePermission: vi.fn().mockResolvedValue(false),
      openSettings: vi.fn().mockRejectedValue(new Error('opener unavailable')),
    });
    const res = await applyToggle(true, false, deps);
    expect(res.status).toBe('denied');
    expect(deps.persist).toHaveBeenCalledWith(true);
  });
});
