// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const values = new Map<string, unknown>();
const get = vi.fn((key: string) => Promise.resolve(values.get(key)));
const set = vi.fn((key: string, value: unknown) => {
  values.set(key, value);
  return Promise.resolve();
});

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() => Promise.resolve({ get, set })),
}));

import {
  listAudioInputDevices,
  loadAudioInputPreference,
  resolveAudioInputDeviceId,
  saveAudioInputPreference,
  watchAudioInputDevices,
} from './useAudioInputDevices';

const enumerateDevices = vi.fn<() => Promise<MediaDeviceInfo[]>>();
const addEventListener = vi.fn();
const removeEventListener = vi.fn();

function device(kind: MediaDeviceKind, deviceId: string, label: string): MediaDeviceInfo {
  return { kind, deviceId, label, groupId: '', toJSON: () => ({}) };
}

beforeEach(() => {
  values.clear();
  vi.clearAllMocks();
  enumerateDevices.mockResolvedValue([]);
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: { enumerateDevices, addEventListener, removeEventListener },
  });
});

describe('Windows audio input preferences', () => {
  it('lists concrete audio inputs and omits output/default pseudo-devices', async () => {
    enumerateDevices.mockResolvedValue([
      device('audioinput', 'default', 'Default - USB Mic'),
      device('audiooutput', 'speaker', 'Speakers'),
      device('audioinput', 'usb', 'USB Mic'),
      device('audioinput', 'built-in', ''),
      device('audioinput', 'usb', 'USB Mic duplicate'),
    ]);

    await expect(listAudioInputDevices()).resolves.toEqual([
      { deviceId: 'usb', label: 'USB Mic' },
      { deviceId: 'built-in', label: 'Microphone 2' },
    ]);
  });

  it('persists an explicit selection and clears back to System default', async () => {
    await saveAudioInputPreference({ deviceId: 'usb', label: 'USB Mic' });
    await expect(loadAudioInputPreference()).resolves.toEqual({
      deviceId: 'usb',
      label: 'USB Mic',
    });

    await saveAudioInputPreference(null);
    await expect(loadAudioInputPreference()).resolves.toEqual({
      deviceId: null,
      label: null,
    });
  });

  it('uses an available saved device and falls back without erasing a missing one', async () => {
    values.set('recordingInputDeviceId', 'usb');
    values.set('recordingInputDeviceLabel', 'USB Mic');
    enumerateDevices.mockResolvedValue([device('audioinput', 'usb', 'USB Mic')]);
    await expect(resolveAudioInputDeviceId()).resolves.toBe('usb');

    enumerateDevices.mockResolvedValue([device('audioinput', 'built-in', 'Laptop Mic')]);
    await expect(resolveAudioInputDeviceId()).resolves.toBeNull();
    await expect(loadAudioInputPreference()).resolves.toEqual({
      deviceId: 'usb',
      label: 'USB Mic',
    });
  });

  it('subscribes to device changes and removes the same listener', () => {
    const callback = vi.fn();
    const stop = watchAudioInputDevices(callback);
    expect(addEventListener).toHaveBeenCalledWith('devicechange', callback);

    stop();
    expect(removeEventListener).toHaveBeenCalledWith('devicechange', callback);
  });
});
