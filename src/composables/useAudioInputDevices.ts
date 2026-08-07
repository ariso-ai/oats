import { load } from '@tauri-apps/plugin-store';
import { listMicrophoneInputDevices } from '../tauri';

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

export class SelectedAudioInputUnavailableError extends Error {
  override name = 'SelectedAudioInputUnavailableError';

  constructor(label?: string | null) {
    super(
      label
        ? `The selected microphone “${label}” is unavailable.`
        : 'The selected microphone is unavailable.',
    );
  }
}

/** Chromium used to expose `default` and `communications` pseudo-endpoints
 * with these label prefixes. Strip only that known legacy decoration when
 * migrating a saved browser choice to a unique native Windows endpoint. */
function comparableAudioInputLabel(label: string): string {
  return label.trim().replace(/^(?:Default|Communications)\s*-\s*/i, '');
}

async function settingsStore() {
  return load('settings.json', { autoSave: true });
}

/** List concrete native Windows capture endpoints. System default is a
 * permanent Settings option rather than a synthetic endpoint in this list. */
export async function listAudioInputDevices(): Promise<AudioInputDevice[]> {
  const devices = await listMicrophoneInputDevices();
  return devices.map((device) => ({
    deviceId: device.deviceId,
    label: device.label,
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

/** Resolve the device ID for the next Windows capture. Native endpoint IDs can
 * rotate when a Bluetooth endpoint reconnects, so a unique exact-label match
 * repairs the stored ID. An explicit selection is never silently replaced by
 * System default: that could capture an unrelated microphone. */
export async function resolveAudioInputDeviceId(): Promise<string | null> {
  const preference = await loadAudioInputPreference();
  if (!preference.deviceId) return null;

  let devices: AudioInputDevice[];
  try {
    devices = await listAudioInputDevices();
  } catch {
    // If discovery itself is unavailable, still try the explicit saved choice;
    // getUserMedia will provide the authoritative capture error.
    return preference.deviceId;
  }

  if (devices.some((device) => device.deviceId === preference.deviceId)) {
    return preference.deviceId;
  }

  if (preference.label) {
    const comparableSavedLabel = comparableAudioInputLabel(preference.label);
    const labelMatches = devices.filter(
      (device) => comparableAudioInputLabel(device.label) === comparableSavedLabel,
    );
    if (labelMatches.length === 1) {
      const replacement = labelMatches[0];
      await saveAudioInputPreference(replacement);
      return replacement.deviceId;
    }
  }

  throw new SelectedAudioInputUnavailableError(preference.label);
}

export function watchAudioInputDevices(onChange: () => void): () => void {
  const timer = window.setInterval(onChange, 2_000);
  return () => window.clearInterval(timer);
}
