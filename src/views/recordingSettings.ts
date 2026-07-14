/** Which recorder pipeline to run. */
export type RecordingMode = 'mic' | 'system' | 'mic_and_system';

/** UI status shown under a toggle after a permission request. */
export type PermissionStatus = '' | 'granted' | 'denied';

/** The two independent recording-source toggles. */
export interface RecordingEnabled {
  mic: boolean;
  systemAudio: boolean;
}

/**
 * Derive the two booleans from a legacy `recordingMode` value (one-time
 * migration). Anything other than the explicit mic-only value — including an
 * absent/unknown value — falls back to the historical default of both on.
 */
export function deriveEnabledFromLegacy(legacy: string | undefined | null): RecordingEnabled {
  if (legacy === 'mic') return { mic: true, systemAudio: false };
  return { mic: true, systemAudio: true };
}

/** Map the two toggles to a recorder mode, or null when neither is enabled. */
export function deriveRecordingMode(enabled: RecordingEnabled): RecordingMode | null {
  if (enabled.mic && enabled.systemAudio) return 'mic_and_system';
  if (enabled.mic) return 'mic';
  if (enabled.systemAudio) return 'system';
  return null;
}

/** Status to show after a permission request resolved. */
export function permissionStatus(granted: boolean): PermissionStatus {
  return granted ? 'granted' : 'denied';
}

/** Injected side effects for a toggle change, so the orchestration stays pure. */
export interface ToggleDeps {
  ensurePermission: () => Promise<boolean>;
  openSettings: () => Promise<void>;
  persist: (enabled: boolean) => Promise<void>;
}

/** Final toggle ref value (after any revert) plus the status to display. */
export interface ToggleResult {
  enabled: boolean;
  status: PermissionStatus;
}

/**
 * Orchestrate a recording toggle change: when turning on, request the OS
 * permission and deep-link to System Settings if it isn't granted; then persist.
 * If persisting fails, revert to `previous`. Permission/settings failures never
 * abort persistence (they are reported as 'denied').
 */
export async function applyToggle(
  checked: boolean,
  previous: boolean,
  deps: ToggleDeps,
): Promise<ToggleResult> {
  let status: PermissionStatus = '';
  if (checked) {
    try {
      const granted = await deps.ensurePermission();
      status = permissionStatus(granted);
      if (!granted) await deps.openSettings();
    } catch {
      status = 'denied';
    }
  }
  try {
    await deps.persist(checked);
    return { enabled: checked, status };
  } catch {
    return { enabled: previous, status };
  }
}
