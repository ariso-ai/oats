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

  it('uses an available saved device', async () => {
    values.set('recordingInputDeviceId', 'usb');
    values.set('recordingInputDeviceLabel', 'USB Mic');
    enumerateDevices.mockResolvedValue([device('audioinput', 'usb', 'USB Mic')]);
    await expect(resolveAudioInputDeviceId()).resolves.toBe('usb');
  });

  it('rebinds a saved device when WebView2 rotates its ID but keeps a unique label', async () => {
    values.set('recordingInputDeviceId', 'old-airpods-id');
    values.set('recordingInputDeviceLabel', 'Headset (AirPods) (Bluetooth)');
    enumerateDevices.mockResolvedValue([
      device('audioinput', 'new-airpods-id', 'Headset (AirPods) (Bluetooth)'),
      device('audioinput', 'webcam', 'C920 Microphone'),
    ]);

    await expect(resolveAudioInputDeviceId()).resolves.toBe('new-airpods-id');
    await expect(loadAudioInputPreference()).resolves.toEqual({
      deviceId: 'new-airpods-id',
      label: 'Headset (AirPods) (Bluetooth)',
    });
  });

  it('does not silently substitute the default for a missing saved device', async () => {
    values.set('recordingInputDeviceId', 'usb');
    values.set('recordingInputDeviceLabel', 'USB Mic');

    enumerateDevices.mockResolvedValue([device('audioinput', 'built-in', 'Laptop Mic')]);
    await expect(resolveAudioInputDeviceId()).rejects.toMatchObject({
      name: 'SelectedAudioInputUnavailableError',
    });
    await expect(loadAudioInputPreference()).resolves.toEqual({
      deviceId: 'usb',
      label: 'USB Mic',
    });
  });

  it('does not guess when multiple current devices have the saved label', async () => {
    values.set('recordingInputDeviceId', 'old-airpods-id');
    values.set('recordingInputDeviceLabel', 'Headset (AirPods) (Bluetooth)');
    enumerateDevices.mockResolvedValue([
      device('audioinput', 'airpods-1', 'Headset (AirPods) (Bluetooth)'),
      device('audioinput', 'airpods-2', 'Headset (AirPods) (Bluetooth)'),
    ]);

    await expect(resolveAudioInputDeviceId()).rejects.toMatchObject({
      name: 'SelectedAudioInputUnavailableError',
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
