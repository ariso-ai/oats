import { load } from '@tauri-apps/plugin-store';

const DEVICE_ID_KEY = 'recordingInputDeviceId';
const DEVICE_LABEL_KEY = 'recordingInputDeviceLabel';

export interface AudioInputDevice {
  deviceId: string;
  label: string;
}

export interface AudioInputPreference {
  deviceId: string | null;
  label: string | null;
}

async function settingsStore() {
  return load('settings.json', { autoSave: true });
}

/** List concrete microphone endpoints. The browser's `default` pseudo-device
 * is represented by the permanent System default option in Settings instead. */
export async function listAudioInputDevices(): Promise<AudioInputDevice[]> {
  const devices = await navigator.mediaDevices.enumerateDevices();
  const seen = new Set<string>();
  const inputs = devices.filter(
    (device) =>
      device.kind === 'audioinput' &&
      device.deviceId !== '' &&
      device.deviceId !== 'default' &&
      !seen.has(device.deviceId) &&
      seen.add(device.deviceId),
  );

  return inputs.map((device, index) => ({
    deviceId: device.deviceId,
    label: device.label.trim() || `Microphone ${index + 1}`,
  }));
}

export async function loadAudioInputPreference(): Promise<AudioInputPreference> {
  const store = await settingsStore();
  const [deviceId, label] = await Promise.all([
    store.get<unknown>(DEVICE_ID_KEY),
    store.get<unknown>(DEVICE_LABEL_KEY),
  ]);
  return {
    deviceId: typeof deviceId === 'string' && deviceId !== '' ? deviceId : null,
    label: typeof label === 'string' && label !== '' ? label : null,
  };
}

export async function saveAudioInputPreference(
  device: AudioInputDevice | null,
): Promise<void> {
  const store = await settingsStore();
  await Promise.all([
    store.set(DEVICE_ID_KEY, device?.deviceId ?? null),
    store.set(DEVICE_LABEL_KEY, device?.label ?? null),
  ]);
}

/** Resolve the device ID for the next Windows capture. A disconnected saved
 * endpoint deliberately falls back without erasing the preference, allowing a
 * later reconnect to restore it and Settings to keep showing the warning. */
export async function resolveAudioInputDeviceId(): Promise<string | null> {
  const preference = await loadAudioInputPreference();
  if (!preference.deviceId) return null;

  try {
    const devices = await listAudioInputDevices();
    return devices.some((device) => device.deviceId === preference.deviceId)
      ? preference.deviceId
      : null;
  } catch {
    // If discovery itself is unavailable, still try the explicit saved choice;
    // getUserMedia will provide the authoritative capture error.
    return preference.deviceId;
  }
}

export function watchAudioInputDevices(onChange: () => void): () => void {
  navigator.mediaDevices.addEventListener('devicechange', onChange);
  return () => navigator.mediaDevices.removeEventListener('devicechange', onChange);
}
