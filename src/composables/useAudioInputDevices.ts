import { load } from '@tauri-apps/plugin-store';
import { listMicrophoneInputDevices } from '../tauri';

const PREFERENCE_KEY = 'recordingInputPreference';
// Legacy keys written before the preference was consolidated into one
// atomic object; read-only, kept so existing installs migrate on load.
const LEGACY_DEVICE_ID_KEY = 'recordingInputDeviceId';
const LEGACY_DEVICE_LABEL_KEY = 'recordingInputDeviceLabel';

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

function asAudioInputPreference(value: unknown): AudioInputPreference | null {
  if (typeof value !== 'object' || value === null) return null;
  const { deviceId, label } = value as Record<string, unknown>;
  if (typeof deviceId !== 'string' && deviceId !== null) return null;
  return {
    deviceId: deviceId ?? null,
    label: typeof label === 'string' ? label : null,
  };
}

export async function loadAudioInputPreference(): Promise<AudioInputPreference> {
  const store = await settingsStore();
  const stored = asAudioInputPreference(await store.get<unknown>(PREFERENCE_KEY));
  if (stored) return stored;

  const [legacyDeviceId, legacyLabel] = await Promise.all([
    store.get<unknown>(LEGACY_DEVICE_ID_KEY),
    store.get<unknown>(LEGACY_DEVICE_LABEL_KEY),
  ]);
  const migrated: AudioInputPreference = {
    deviceId:
      typeof legacyDeviceId === 'string' && legacyDeviceId !== '' ? legacyDeviceId : null,
    label: typeof legacyLabel === 'string' && legacyLabel !== '' ? legacyLabel : null,
  };
  if (migrated.deviceId) {
    await store.set(PREFERENCE_KEY, migrated);
  }
  return migrated;
}

export async function saveAudioInputPreference(
  device: AudioInputDevice | null,
): Promise<void> {
  const store = await settingsStore();
  await store.set(PREFERENCE_KEY, {
    deviceId: device?.deviceId ?? null,
    label: device?.label ?? null,
  });
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
