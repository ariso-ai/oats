// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const values = new Map<string, unknown>();
const get = vi.fn((key: string) => Promise.resolve(values.get(key)));
const set = vi.fn((key: string, value: unknown) => {
  values.set(key, value);
  return Promise.resolve();
});

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() => Promise.resolve({ get, set })),
}));

const listMicrophoneInputDevices = vi.fn(() => Promise.resolve([] as Array<{
  deviceId: string;
  label: string;
  isDefault: boolean;
}>));

vi.mock('../tauri', () => ({
  listMicrophoneInputDevices: () => listMicrophoneInputDevices(),
}));

import {
  listAudioInputDevices,
  loadAudioInputPreference,
  resolveAudioInputDeviceId,
  saveAudioInputPreference,
  watchAudioInputDevices,
} from './useAudioInputDevices';

function device(deviceId: string, label: string, isDefault = false) {
  return { deviceId, label, isDefault };
}

beforeEach(() => {
  values.clear();
  vi.clearAllMocks();
  listMicrophoneInputDevices.mockResolvedValue([]);
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('Windows audio input preferences', () => {
  it('lists concrete native Windows capture endpoints', async () => {
    listMicrophoneInputDevices.mockResolvedValue([
      device('{usb}', 'USB Mic', true),
      device('{airpods}', 'Headset (AirPods) (Bluetooth)'),
    ]);

    await expect(listAudioInputDevices()).resolves.toEqual([
      { deviceId: '{usb}', label: 'USB Mic' },
      { deviceId: '{airpods}', label: 'Headset (AirPods) (Bluetooth)' },
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
    listMicrophoneInputDevices.mockResolvedValue([device('usb', 'USB Mic')]);
    await expect(resolveAudioInputDeviceId()).resolves.toBe('usb');
  });

  it('migrates a legacy browser communications choice to one native endpoint', async () => {
    values.set('recordingInputDeviceId', 'old-airpods-id');
    values.set(
      'recordingInputDeviceLabel',
      'Communications - Headset (AirPods) (Bluetooth)',
    );
    listMicrophoneInputDevices.mockResolvedValue([
      device('new-airpods-id', 'Headset (AirPods) (Bluetooth)'),
      device('webcam', 'C920 Microphone'),
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

    listMicrophoneInputDevices.mockResolvedValue([device('built-in', 'Laptop Mic')]);
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
    listMicrophoneInputDevices.mockResolvedValue([
      device('airpods-1', 'Headset (AirPods) (Bluetooth)'),
      device('airpods-2', 'Headset (AirPods) (Bluetooth)'),
    ]);

    await expect(resolveAudioInputDeviceId()).rejects.toMatchObject({
      name: 'SelectedAudioInputUnavailableError',
    });
  });

  it('polls for native device changes and stops polling on cleanup', () => {
    const callback = vi.fn();
    const stop = watchAudioInputDevices(callback);

    vi.advanceTimersByTime(2_000);
    expect(callback).toHaveBeenCalledOnce();

    stop();
    vi.advanceTimersByTime(4_000);
    expect(callback).toHaveBeenCalledOnce();
  });
});
