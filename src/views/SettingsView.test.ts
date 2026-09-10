// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));
const emit = vi.fn((..._a: unknown[]) => Promise.resolve());
const getBackendSetting = vi.fn(() => Promise.resolve('ariso' as const));
const setBackendSetting = vi.fn((_b: unknown) => Promise.resolve());
const hasPromptedLocalModels = vi.fn(() => Promise.resolve(false));
const setPromptedLocalModels = vi.fn((_v: unknown) => Promise.resolve());
const downloadStt = vi.fn(() => Promise.resolve());
const downloadLlm = vi.fn(() => Promise.resolve());
const checkSession = vi.fn((): Promise<unknown> => Promise.resolve(null));
const apiRequest = vi.fn(
  (
    _method: string,
    _path: string,
    _body?: unknown
  ): Promise<{ status: number; data: unknown }> =>
    Promise.resolve({ status: 200, data: {} })
);
const getVaultDir = vi.fn(() => Promise.resolve('/Users/x/.ariso/vault'));
const setVaultDir = vi.fn((_path: string) => Promise.resolve());
const pickVaultFolder = vi.fn(
  (_current?: string): Promise<string | null> => Promise.resolve(null)
);
type SignInResult = { success?: boolean; sessionToken?: string; error?: string };
const googleSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true, sessionToken: 't' }));
const microsoftSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true, sessionToken: 't' }));
const cancelSignIn = vi.fn(() => Promise.resolve());
const ensureCalendarAccess = vi.fn(
  (): Promise<{ connected: boolean; reason?: string }> => Promise.resolve({ connected: true })
);
const signOut = vi.fn(() => Promise.resolve());
const emitNotificationsSync = vi.fn(() => Promise.resolve());

// Capture event listeners by name so tests can fire them.
const listeners = new Map<string, (e: { payload: unknown }) => void>();

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getAllWebviewWindows: () => getAllWebviewWindows(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
  emit: (...args: unknown[]) => emit(...args),
}));
vi.mock('../tauri', () => ({
  AUTH_SIGNED_IN_EVENT: 'auth://signed-in',
  SIGN_IN_CANCELED_ERROR: 'Sign-in canceled',
  auth: {
    checkSession: () => checkSession(),
    googleSignIn: () => googleSignIn(),
    microsoftSignIn: () => microsoftSignIn(),
    cancelSignIn: () => cancelSignIn(),
    ensureCalendarAccess: () => ensureCalendarAccess(),
    signOut: () => signOut(),
  },
  api: {
    request: (method: string, path: string, body?: unknown) =>
      apiRequest(method, path, body),
  },
  updater: {
    getState: () =>
      Promise.resolve({
        auto_check_enabled: true,
        last_check_unix: null,
        skipped_version: null,
        latest_known: null,
      }),
    check: vi.fn(),
    setAutoCheck: vi.fn(),
  },
  getBackendSetting: () => getBackendSetting(),
  setBackendSetting: (b: unknown) => setBackendSetting(b),
  hasPromptedLocalModels: () => hasPromptedLocalModels(),
  setPromptedLocalModels: (v: unknown) => setPromptedLocalModels(v),
  getVaultDir: () => getVaultDir(),
  setVaultDir: (path: string) => setVaultDir(path),
  pickVaultFolder: (current?: string) => pickVaultFolder(current),
  local: {
    modelStatus: () => Promise.resolve({ state: 'not_downloaded' }),
    downloadStt: () => downloadStt(),
    downloadLlm: () => downloadLlm(),
  },
}));
vi.mock('../composables/useRecordingPermissions', () => ({
  loadRecordingEnabled: () => Promise.resolve({ mic: false, systemAudio: false }),
  setMicEnabled: vi.fn(),
  setSystemAudioEnabled: vi.fn(),
  ensureMicPermission: vi.fn(),
  ensureSystemAudioPermission: vi.fn(),
  checkSystemAudioPermission: vi.fn(() => Promise.resolve(true)),
  openMicSettings: vi.fn(),
  openSystemAudioSettings: vi.fn(),
}));
vi.mock('../composables/useMeetingNotifications', () => ({
  isMeetingNotificationsEnabled: () => Promise.resolve(false),
  setMeetingNotificationsEnabled: vi.fn(),
  ensureNotificationPermission: vi.fn(),
  openNotificationSettings: vi.fn(),
  emitNotificationsSync: () => emitNotificationsSync(),
}));
vi.mock('../composables/useAutoRecord', () => ({
  isAutoRecordEnabled: () => Promise.resolve(false),
  setAutoRecordEnabled: vi.fn(),
  isAutoRecordSupported: () => Promise.resolve(true),
}));
vi.mock('../composables/usePlatformCapabilities', () => {
  const capabilities = () => ({
    os: 'macos',
    localBackend: { supported: true, engine: 'swift-mlx' },
    systemAudio: {
      supported: true,
      settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture',
    },
    autoRecord: { supported: true },
    nativeShare: { supported: true },
    notificationSettingsUrl: 'x-apple.systempreferences:com.apple.Notifications-Settings.extension',
    microphoneSettingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone',
  });
  return {
    defaultPlatformCapabilities: capabilities,
    loadPlatformCapabilities: () => Promise.resolve(capabilities()),
  };
});
const setSilenceDetectionEnabled = vi.fn(() => Promise.resolve());
vi.mock('../composables/useSilenceDetection', () => ({
  isSilenceDetectionEnabled: () => Promise.resolve(true),
  setSilenceDetectionEnabled: (...a: unknown[]) => setSilenceDetectionEnabled(...a),
}));
const setMeetingEndReminderEnabled = vi.fn(() => Promise.resolve());
vi.mock('../composables/useMeetingEndReminder', () => ({
  isMeetingEndReminderEnabled: () => Promise.resolve(true),
  setMeetingEndReminderEnabled: (...a: unknown[]) => setMeetingEndReminderEnabled(...a),
}));
const isDiagnosticsEnabled = vi.fn(() => Promise.resolve(false));
const setDiagnosticsEnabled = vi.fn((_v: unknown) => Promise.resolve());
vi.mock('../composables/useDiagnostics', () => ({
  isDiagnosticsEnabled: () => isDiagnosticsEnabled(),
  setDiagnosticsEnabled: (v: unknown) => setDiagnosticsEnabled(v),
}));

import SettingsView from './SettingsView.vue';

// Remove each component's window 'focus' listener between tests.
enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  getAllWebviewWindows.mockResolvedValue([]);
  // clearAllMocks keeps the last-set implementation, so restore the signed-out
  // defaults for every test; the avatar suite overrides these explicitly.
  checkSession.mockResolvedValue(null);
  apiRequest.mockImplementation(() =>
    Promise.resolve({ status: 200, data: {} })
  );
  getBackendSetting.mockResolvedValue('ariso');
  isDiagnosticsEnabled.mockResolvedValue(false);
  getVaultDir.mockResolvedValue('/Users/x/.ariso/vault');
  setVaultDir.mockResolvedValue(undefined);
  pickVaultFolder.mockResolvedValue(null);
  googleSignIn.mockResolvedValue({ success: true, sessionToken: 't' });
  microsoftSignIn.mockResolvedValue({ success: true, sessionToken: 't' });
  cancelSignIn.mockResolvedValue(undefined);
  ensureCalendarAccess.mockResolvedValue({ connected: true });
  signOut.mockResolvedValue(undefined);
});

function fireRecordingState(active: boolean) {
  const cb = listeners.get('recording://state');
  expect(cb).toBeDefined();
  cb!({ payload: active });
}

describe('SettingsView backend switching during recording', () => {
  it('enables the backend trigger when no recording is active', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const trigger = wrapper.get('.backend-trigger');
    expect(trigger.attributes('disabled')).toBeUndefined();
    expect(wrapper.text()).not.toContain("Backend can't be changed while recording.");
  });

  it('disables the trigger and shows a hint when the waveform window exists', async () => {
    getAllWebviewWindows.mockResolvedValue([{ label: 'waveform' }]);
    const wrapper = mount(SettingsView);
    await flushPromises();
    const trigger = wrapper.get('.backend-trigger');
    expect(trigger.attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain("Backend can't be changed while recording.");
  });

  it('reacts live to recording://state events', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    fireRecordingState(true);
    await flushPromises();
    expect(wrapper.get('.backend-trigger').attributes('disabled')).toBeDefined();

    fireRecordingState(false);
    await flushPromises();
    expect(wrapper.get('.backend-trigger').attributes('disabled')).toBeUndefined();
  });

  it('closes an open backend menu when a recording starts', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.get('.backend-trigger').trigger('click');
    expect(wrapper.find('.backend-menu').exists()).toBe(true);

    fireRecordingState(true);
    await flushPromises();
    expect(wrapper.find('.backend-menu').exists()).toBe(false);
  });

  it('re-checks the waveform window on window focus', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.get('.backend-trigger').attributes('disabled')).toBeUndefined();

    getAllWebviewWindows.mockResolvedValue([{ label: 'waveform' }]);
    window.dispatchEvent(new Event('focus'));
    await flushPromises();
    expect(wrapper.get('.backend-trigger').attributes('disabled')).toBeDefined();
  });

  it('ignores a backend selection that lands as recording starts', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.get('.backend-trigger').trigger('click');
    // Recording starts while the menu is still rendered: fire the event but
    // don't flush, so the option below is clicked before the menu reacts.
    fireRecordingState(true);
    await wrapper.findAll('.backend-option')[1].trigger('mousedown');
    await flushPromises();

    expect(setBackendSetting).not.toHaveBeenCalled();
  });

  it('removes the recording://state listener on unmount', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(listeners.has('recording://state')).toBe(true);
    wrapper.unmount();
    expect(listeners.has('recording://state')).toBe(false);
  });
});

describe('SettingsView first-time local models prompt', () => {
  async function switchToLocal(wrapper: ReturnType<typeof mount>) {
    await wrapper.get('.backend-trigger').trigger('click');
    // The Local option is the second backend option.
    await wrapper.findAll('.backend-option')[1].trigger('mousedown');
    await flushPromises();
  }

  it('opens the confirm modal on first switch to Local', async () => {
    hasPromptedLocalModels.mockResolvedValue(false);
    const wrapper = mount(SettingsView);
    await flushPromises();

    await switchToLocal(wrapper);

    expect(wrapper.find('.download-confirm').exists()).toBe(true);
    expect(wrapper.text()).toContain('Download on-device models');
  });

  it('broadcasts a backend-changed event so other windows can react', async () => {
    // Already-prompted so the switch runs clean without the download modal.
    hasPromptedLocalModels.mockResolvedValue(true);
    const wrapper = mount(SettingsView);
    await flushPromises();

    await switchToLocal(wrapper);

    expect(emit).toHaveBeenCalledWith('backend://changed');
  });

  it('downloads both models and persists the flag on confirm', async () => {
    hasPromptedLocalModels.mockResolvedValue(false);
    const wrapper = mount(SettingsView);
    await flushPromises();
    await switchToLocal(wrapper);

    await wrapper.get('.download-confirm__confirm').trigger('click');
    await flushPromises();

    expect(downloadStt).toHaveBeenCalledTimes(1);
    expect(downloadLlm).toHaveBeenCalledTimes(1);
    expect(setPromptedLocalModels).toHaveBeenCalledWith(true);
    expect(wrapper.find('.download-confirm').exists()).toBe(false);
  });

  it('reverts to Ariso and does not download on cancel', async () => {
    hasPromptedLocalModels.mockResolvedValue(false);
    const wrapper = mount(SettingsView);
    await flushPromises();
    await switchToLocal(wrapper);

    setBackendSetting.mockClear();
    await wrapper.get('.download-confirm__cancel').trigger('click');
    await flushPromises();

    expect(setBackendSetting).toHaveBeenCalledWith('ariso');
    expect(downloadStt).not.toHaveBeenCalled();
    expect(setPromptedLocalModels).not.toHaveBeenCalled();
    expect(wrapper.find('.download-confirm').exists()).toBe(false);
  });

  it('skips the modal but auto-starts missing downloads when already prompted', async () => {
    hasPromptedLocalModels.mockResolvedValue(true);
    const wrapper = mount(SettingsView);
    await flushPromises();
    await switchToLocal(wrapper);

    // No modal the second time, but the still-missing models download right away.
    expect(wrapper.find('.download-confirm').exists()).toBe(false);
    expect(downloadStt).toHaveBeenCalledTimes(1);
    expect(downloadLlm).toHaveBeenCalledTimes(1);
  });
});

describe('SettingsView account avatar', () => {
  // The component preloads the avatar through a detached `new Image()` before
  // binding it to the <img>. Stub Image so tests control load success/failure.
  let imageShouldFail = false;
  class FakeImage {
    referrerPolicy = '';
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(_v: string) {
      queueMicrotask(() => {
        if (imageShouldFail) this.onerror?.();
        else this.onload?.();
      });
    }
  }
  beforeEach(() => {
    imageShouldFail = false;
    vi.stubGlobal('Image', FakeImage);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  // Sign the user in and route the profile calls fetchUserProfile makes.
  function mockSignedIn(avatar: string | null, microsoftAvatar: string | null = null) {
    checkSession.mockResolvedValue({ token: 'session' });
    apiRequest.mockImplementation((_method: string, path: string) => {
      if (path === '/auth/me') {
        return Promise.resolve({
          status: 200,
          data: { full_name: 'Ada Lovelace', email: 'ada@example.com' },
        });
      }
      if (path === '/users/google-avatar') {
        return Promise.resolve({
          status: 200,
          data: { avatar, connected: avatar != null },
        });
      }
      if (path === '/users/microsoft-avatar') {
        return Promise.resolve({
          status: 200,
          data: { avatar: microsoftAvatar, connected: microsoftAvatar != null },
        });
      }
      return Promise.resolve({ status: 200, data: {} });
    });
  }

  it('falls back to the Microsoft avatar when there is no Google avatar', async () => {
    mockSignedIn(null, 'https://example.com/microsoft-photo.png');
    const wrapper = mount(SettingsView);
    await flushPromises();

    const img = wrapper.find('img.avatar');
    expect(img.exists()).toBe(true);
    expect(img.attributes('src')).toBe('https://example.com/microsoft-photo.png');
  });

  it('does not ask for the Microsoft avatar when a Google avatar exists', async () => {
    mockSignedIn('https://lh3.googleusercontent.com/a/photo.png');
    mount(SettingsView);
    await flushPromises();

    const paths = apiRequest.mock.calls.map((call) => call[1]);
    expect(paths).toContain('/users/google-avatar');
    expect(paths).not.toContain('/users/microsoft-avatar');
  });

  it('renders the Google avatar image when one is available', async () => {
    mockSignedIn('https://lh3.googleusercontent.com/a/photo.png');
    const wrapper = mount(SettingsView);
    await flushPromises();

    const img = wrapper.find('img.avatar');
    expect(img.exists()).toBe(true);
    expect(img.attributes('src')).toBe(
      'https://lh3.googleusercontent.com/a/photo.png'
    );
    // The initials circle should not also render.
    expect(wrapper.find('div.avatar').exists()).toBe(false);
  });

  it('hides the sign-in button once signed in', async () => {
    // Regression: the Connect Calendar block was inserted between the
    // signed-in v-if and the sign-in v-else, which silently re-paired the
    // v-else to it — so the sign-in button rendered next to Sign Out.
    mockSignedIn(null);
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find('.sign-in-container').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('Sign in with Google');
    expect(wrapper.text()).not.toContain('Sign in with Microsoft');
    expect(wrapper.text()).toContain('Sign Out');
  });

  it('shows the sign-in button when signed out', async () => {
    checkSession.mockResolvedValue(null);
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find('.sign-in-container').exists()).toBe(true);
    expect(wrapper.text()).toContain('Sign in with Google');
  });

  it('falls back to the initials circle when there is no Google avatar', async () => {
    mockSignedIn(null);
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find('img.avatar').exists()).toBe(false);
    const initialsCircle = wrapper.find('div.avatar');
    expect(initialsCircle.exists()).toBe(true);
    expect(initialsCircle.text()).toBe('AD');
  });

  it('still shows initials when the avatar request fails', async () => {
    checkSession.mockResolvedValue({ token: 'session' });
    apiRequest.mockImplementation((_method: string, path: string) => {
      if (path === '/auth/me') {
        return Promise.resolve({
          status: 200,
          data: { full_name: 'Ada Lovelace', email: 'ada@example.com' },
        });
      }
      if (path === '/users/google-avatar') {
        return Promise.reject(new Error('network'));
      }
      return Promise.resolve({ status: 200, data: {} });
    });
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find('img.avatar').exists()).toBe(false);
    expect(wrapper.find('div.avatar').text()).toBe('AD');
    // The avatar failure must not wipe out the name/email that loaded first.
    expect(wrapper.text()).toContain('Ada Lovelace');
    expect(wrapper.text()).toContain('ada@example.com');
  });

  it('falls back to initials when the avatar image never loads', async () => {
    imageShouldFail = true;
    mockSignedIn('https://lh3.googleusercontent.com/a/photo.png');
    vi.useFakeTimers();
    try {
      const wrapper = mount(SettingsView);
      // Drives onMounted, the profile fetch, and the preload retry backoff.
      await vi.runAllTimersAsync();
      expect(wrapper.find('img.avatar').exists()).toBe(false);
      expect(wrapper.find('div.avatar').text()).toBe('AD');
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('SettingsView sign-in providers', () => {
  async function mountSignedOut() {
    const wrapper = mount(SettingsView);
    await flushPromises();
    return wrapper;
  }

  it('offers Google and Microsoft sign-in when signed out', async () => {
    const wrapper = await mountSignedOut();
    expect(wrapper.get('.google-btn').text()).toContain('Sign in with Google');
    expect(wrapper.get('.microsoft-btn').text()).toContain('Sign in with Microsoft');
  });

  it('Microsoft sign-in syncs notifications and never runs the Google calendar hop', async () => {
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(microsoftSignIn).toHaveBeenCalledTimes(1);
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(emitNotificationsSync).toHaveBeenCalled();
    // ensureCalendarAccess opens Google's Workspace consent page.
    expect(ensureCalendarAccess).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('Sign Out');
    expect(wrapper.find('.calendar-connect').exists()).toBe(false);
  });

  it('Google sign-in still runs the calendar hop', async () => {
    const wrapper = await mountSignedOut();
    await wrapper.get('.google-btn').trigger('click');
    await flushPromises();

    expect(googleSignIn).toHaveBeenCalledTimes(1);
    expect(ensureCalendarAccess).toHaveBeenCalledTimes(1);
  });

  it('disables both buttons while either flow is pending and relabels only the clicked one', async () => {
    microsoftSignIn.mockReturnValue(new Promise(() => {}));
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    const google = wrapper.get('.google-btn');
    const microsoft = wrapper.get('.microsoft-btn');
    expect(google.attributes('disabled')).toBeDefined();
    expect(microsoft.attributes('disabled')).toBeDefined();
    expect(microsoft.text()).toContain('Continue in your browser…');
    expect(google.text()).toContain('Sign in with Google');

    // One Cancel covers both providers.
    await wrapper.get('.sign-in-container .sign-out-btn').trigger('click');
    expect(cancelSignIn).toHaveBeenCalledTimes(1);
  });

  it('resets both buttons silently when a Microsoft sign-in is canceled', async () => {
    microsoftSignIn.mockResolvedValue({ error: 'Sign-in canceled' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(wrapper.find('.sign-in-container .error').exists()).toBe(false);
    expect(wrapper.get('.google-btn').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('.microsoft-btn').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('.microsoft-btn').text()).toContain('Sign in with Microsoft');
  });

  it('shows a Microsoft sign-in error under the buttons', async () => {
    microsoftSignIn.mockResolvedValue({ error: 'API returned 500' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(wrapper.get('.sign-in-container .error').text()).toBe('API returned 500');
  });

  it('never shows a Microsoft user the Google calendar nudge left over from a Google session', async () => {
    // The settings window outlives sign-outs, so calendarConnected=false from a
    // Google user must not leak into the next (Microsoft) session.
    ensureCalendarAccess.mockResolvedValue({ connected: false, reason: 'no_calendar_scope' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.google-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.calendar-connect').exists()).toBe(true);

    await wrapper.get('.account-info .sign-out-btn').trigger('click');
    await flushPromises();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('Sign Out');
    expect(wrapper.find('.calendar-connect').exists()).toBe(false);
  });
});

describe('SettingsView silence detection toggle', () => {
  // Find the checkbox in the setting-row whose label is `label`.
  function toggleFor(wrapper: ReturnType<typeof mount>, label: string) {
    const row = wrapper
      .findAll('.setting-row')
      .find((r) => r.find('.setting-label').text() === label);
    expect(row, `setting-row for "${label}"`).toBeDefined();
    return row!.find('input.toggle-input');
  }

  it('renders the Silence detection toggle, checked by default', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Silence detection');
    expect(input.exists()).toBe(true);
    expect((input.element as HTMLInputElement).checked).toBe(true);
  });

  it('persists the new value when toggled off', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Silence detection');
    (input.element as HTMLInputElement).checked = false;
    await input.trigger('change');
    await flushPromises();
    expect(setSilenceDetectionEnabled).toHaveBeenCalledWith(false);
  });
});

describe('SettingsView diagnostics toggle', () => {
  function toggleFor(wrapper: ReturnType<typeof mount>, label: string) {
    const row = wrapper
      .findAll('.setting-row')
      .find((r) => r.find('.setting-label').text() === label);
    expect(row, `setting-row for "${label}"`).toBeDefined();
    return row!.find('input.toggle-input');
  }

  it('renders unchecked by default', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Collect diagnostic data');
    expect(input.exists()).toBe(true);
    expect((input.element as HTMLInputElement).checked).toBe(false);
  });

  it('reflects a stored opt-in', async () => {
    isDiagnosticsEnabled.mockResolvedValue(true);
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(
      (toggleFor(wrapper, 'Collect diagnostic data').element as HTMLInputElement).checked
    ).toBe(true);
  });

  it('persists the opt-in when switched on', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Collect diagnostic data');
    (input.element as HTMLInputElement).checked = true;
    await input.trigger('change');
    await flushPromises();
    expect(setDiagnosticsEnabled).toHaveBeenCalledWith(true);
  });

  it('reverts the toggle when persistence fails', async () => {
    setDiagnosticsEnabled.mockRejectedValueOnce(new Error('store locked'));
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Collect diagnostic data');
    (input.element as HTMLInputElement).checked = true;
    await input.trigger('change');
    await flushPromises();
    expect((input.element as HTMLInputElement).checked).toBe(false);
  });

  it('explains that reporting is paused in offline mode', async () => {
    isDiagnosticsEnabled.mockResolvedValue(true);
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.text()).toContain('Paused while oats is running on-device');
    // Windows ships the local backend too (cpp-sidecar), so the promise this
    // notice makes has to hold on both platforms.
    expect(wrapper.text()).toContain('nothing leaves your device');
    expect(wrapper.text()).not.toContain('your Mac');
  });

  it('shows no offline notice while opted out', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.text()).not.toContain('Paused while oats is running on-device');
  });
});

describe('SettingsView vault location', () => {
  beforeEach(() => {
    // The vault control only renders inside the "On-device models" card,
    // which is shown for the local backend.
    getBackendSetting.mockResolvedValue('local');
  });

  it('shows the current vault path and changes it via the folder picker', async () => {
    getVaultDir.mockResolvedValue('/Users/x/.ariso/vault');
    pickVaultFolder.mockResolvedValue('/Users/x/Notes/oats');

    const wrapper = mount(SettingsView);
    await flushPromises();

    // The full path is carried in the title; the visible text is truncated.
    expect(wrapper.get('[data-test="vault-path"]').attributes('title')).toBe(
      '/Users/x/.ariso/vault'
    );

    await wrapper.find('[data-test="change-vault"]').trigger('click');
    await flushPromises();

    expect(setVaultDir).toHaveBeenCalledWith('/Users/x/Notes/oats');
    expect(wrapper.get('[data-test="vault-path"]').attributes('title')).toBe(
      '/Users/x/Notes/oats'
    );
  });

  it('front-truncates a long path under 20 chars while keeping the full path in the title', async () => {
    getVaultDir.mockResolvedValue('/Users/x/Documents/Notes/oats-vault');

    const wrapper = mount(SettingsView);
    await flushPromises();

    const path = wrapper.get('[data-test="vault-path"]');
    expect(path.attributes('title')).toBe('/Users/x/Documents/Notes/oats-vault');
    const shown = path.text();
    expect(shown.startsWith('...')).toBe(true);
    expect(shown.length).toBeLessThanOrEqual(20);
    // The tail (most specific part of the path) stays visible.
    expect(shown.endsWith('oats-vault')).toBe(true);
  });

  it('exposes the vault description via an accessible help tooltip', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    const help = wrapper.find('[data-test="vault-help"]');
    expect(help.exists()).toBe(true);
    // The button points at the tooltip that carries the description.
    const tooltipId = help.attributes('aria-describedby');
    expect(tooltipId).toBe('vault-help-text');
    const tooltip = wrapper.find(`#${tooltipId}`);
    expect(tooltip.attributes('role')).toBe('tooltip');
    const description = tooltip.text().replace(/\s+/g, ' ');
    expect(description).toContain("existing recordings stay in the old folder and aren't moved");
    expect(description).toContain('those notes and audio leave this device');
  });

  it('does nothing when the folder picker is dismissed', async () => {
    pickVaultFolder.mockResolvedValue(null);
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.find('[data-test="change-vault"]').trigger('click');
    await flushPromises();

    expect(setVaultDir).not.toHaveBeenCalled();
  });

  it('disables the change-vault button while a recording is active', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    fireRecordingState(true);
    await flushPromises();

    expect(
      wrapper.get('[data-test="change-vault"]').attributes('disabled')
    ).toBeDefined();
  });

  it('shows an error and re-syncs the displayed path when setVaultDir fails', async () => {
    getVaultDir
      .mockResolvedValueOnce('/Users/x/.ariso/vault')
      .mockResolvedValueOnce('/Users/x/.ariso/vault-actual');
    pickVaultFolder.mockResolvedValue('/Users/x/Notes/oats');
    setVaultDir.mockRejectedValue(new Error('failed to persist vault dir'));

    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.find('[data-test="change-vault"]').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('failed to persist vault dir');
    // Re-fetches the true active vault rather than trusting the failed picked path.
    expect(getVaultDir).toHaveBeenCalledTimes(2);
    expect(wrapper.get('[data-test="vault-path"]').attributes('title')).toBe(
      '/Users/x/.ariso/vault-actual'
    );
  });
});

describe('SettingsView meeting stop reminder toggle', () => {
  // Find the checkbox in the setting-row whose label is `label`.
  function toggleFor(wrapper: ReturnType<typeof mount>, label: string) {
    const row = wrapper
      .findAll('.setting-row')
      .find((r) => r.find('.setting-label').text() === label);
    expect(row, `setting-row for "${label}"`).toBeDefined();
    return row!.find('input.toggle-input');
  }

  it('renders the Meeting stop reminder toggle, checked by default', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Meeting stop reminder');
    expect(input.exists()).toBe(true);
    expect((input.element as HTMLInputElement).checked).toBe(true);
  });

  it('persists the new value when toggled off', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const input = toggleFor(wrapper, 'Meeting stop reminder');
    (input.element as HTMLInputElement).checked = false;
    await input.trigger('change');
    await flushPromises();
    expect(setMeetingEndReminderEnabled).toHaveBeenCalledWith(false);
  });
});
