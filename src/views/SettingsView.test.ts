// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));
const emit = vi.fn((..._a: unknown[]) => Promise.resolve());
const getBackendSetting = vi.fn(
  (): Promise<'ariso' | 'local'> => Promise.resolve('ariso')
);
const setBackendSetting = vi.fn((_b: unknown) => Promise.resolve());
const hasPromptedLocalModels = vi.fn(() => Promise.resolve(false));
const setPromptedLocalModels = vi.fn((_v: unknown) => Promise.resolve());
type ModelStatusShape = { state: string; version?: string; llmReady?: boolean };
const modelStatus = vi.fn(
  (): Promise<ModelStatusShape> => Promise.resolve({ state: 'not_downloaded' })
);
const modelSizes = vi.fn(
  (): Promise<{ notes: number | null; speech: number | null }> =>
    Promise.resolve({ notes: null, speech: null })
);
const deleteModel = vi.fn((_k: unknown) => Promise.resolve());
const llmApiKeyProviders = vi.fn((): Promise<string[]> => Promise.resolve([]));
const setLlmApiKey = vi.fn((_p: unknown, _k: unknown) => Promise.resolve());
const clearLlmApiKey = vi.fn((_p: unknown) => Promise.resolve());
const downloadStt = vi.fn(() => Promise.resolve());
const downloadLlm = vi.fn(() => Promise.resolve());
const getNotesModelSetting = vi.fn(() =>
  Promise.resolve({ kind: 'local', id: 'gemma-3-1b-it-qat-4bit' } as const)
);
const setNotesModelSetting = vi.fn((_m: unknown) => Promise.resolve());
const getSpeechModelSetting = vi.fn(() => Promise.resolve('speech:parakeet-tdt-0.6b-v3'));
const setSpeechModelSetting = vi.fn((_k: unknown) => Promise.resolve());
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
  AUTH_CHANGED_EVENT: 'auth://changed',
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
  getNotesModelSetting: () => getNotesModelSetting(),
  setNotesModelSetting: (m: unknown) => setNotesModelSetting(m),
  getSpeechModelSetting: () => getSpeechModelSetting(),
  setSpeechModelSetting: (k: unknown) => setSpeechModelSetting(k),
  getVaultDir: () => getVaultDir(),
  setVaultDir: (path: string) => setVaultDir(path),
  pickVaultFolder: (current?: string) => pickVaultFolder(current),
  local: {
    modelStatus: () => modelStatus(),
    downloadStt: () => downloadStt(),
    downloadLlm: () => downloadLlm(),
    modelSizes: () => modelSizes(),
    deleteModel: (k: unknown) => deleteModel(k),
  },
  llmKeys: {
    providers: () => llmApiKeyProviders(),
    set: (p: unknown, k: unknown) => setLlmApiKey(p, k),
    clear: (p: unknown) => clearLlmApiKey(p),
  },
}));
const loadRecordingEnabled = vi.fn(() => Promise.resolve({ mic: false, systemAudio: false }));
const ensureMicPermission = vi.fn(() => Promise.resolve(true));
vi.mock('../composables/useRecordingPermissions', () => ({
  loadRecordingEnabled: () => loadRecordingEnabled(),
  setMicEnabled: vi.fn(),
  setSystemAudioEnabled: vi.fn(),
  ensureMicPermission: () => ensureMicPermission(),
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
  getNotesModelSetting.mockResolvedValue({ kind: 'local', id: 'gemma-3-1b-it-qat-4bit' });
  modelStatus.mockResolvedValue({ state: 'not_downloaded' });
  modelSizes.mockResolvedValue({ notes: null, speech: null });
  getSpeechModelSetting.mockResolvedValue('speech:parakeet-tdt-0.6b-v3');
  deleteModel.mockResolvedValue(undefined);
  llmApiKeyProviders.mockResolvedValue([]);
  setLlmApiKey.mockResolvedValue(undefined);
  clearLlmApiKey.mockResolvedValue(undefined);
  isDiagnosticsEnabled.mockResolvedValue(false);
  getVaultDir.mockResolvedValue('/Users/x/.ariso/vault');
  setVaultDir.mockResolvedValue(undefined);
  pickVaultFolder.mockResolvedValue(null);
  // Sign-in stores a session and sign-out clears it, as the backend does, so
  // the account refresh that follows each one reads the new state.
  googleSignIn.mockImplementation(storeSession);
  microsoftSignIn.mockImplementation(storeSession);
  cancelSignIn.mockResolvedValue(undefined);
  ensureCalendarAccess.mockResolvedValue({ connected: true });
  signOut.mockImplementation(() => {
    checkSession.mockResolvedValue(null);
    return Promise.resolve();
  });
  loadRecordingEnabled.mockResolvedValue({ mic: false, systemAudio: false });
  ensureMicPermission.mockResolvedValue(true);
});

function storeSession(): Promise<SignInResult> {
  checkSession.mockResolvedValue({ sessionToken: 't' });
  return Promise.resolve({ success: true, sessionToken: 't' });
}

function fireAuthChanged() {
  const cb = listeners.get('auth://changed');
  expect(cb).toBeDefined();
  cb!({ payload: null });
}

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
    expect(wrapper.text()).toContain('Download local models');
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
    await wrapper.get('.sign-in-cancel').trigger('click');
    expect(cancelSignIn).toHaveBeenCalledTimes(1);
  });

  it('resets both buttons silently when a Microsoft sign-in is canceled', async () => {
    microsoftSignIn.mockResolvedValue({ error: 'Sign-in canceled' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(wrapper.find('.sign-in-error').exists()).toBe(false);
    expect(wrapper.get('.google-btn').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('.microsoft-btn').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('.microsoft-btn').text()).toContain('Sign in with Microsoft');
  });

  it('shows a Microsoft sign-in error under the buttons', async () => {
    microsoftSignIn.mockResolvedValue({ error: 'API returned 500' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.microsoft-btn').trigger('click');
    await flushPromises();

    expect(wrapper.get('.sign-in-error').text()).toBe('API returned 500');
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

  it('drops the Google calendar nudge on sign-out even when the next sign-in happens in another window', async () => {
    ensureCalendarAccess.mockResolvedValue({ connected: false, reason: 'no_calendar_scope' });
    const wrapper = await mountSignedOut();
    await wrapper.get('.google-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.calendar-connect').exists()).toBe(true);

    await wrapper.get('.account-info .sign-out-btn').trigger('click');
    await flushPromises();

    // A Microsoft sign-in completed in Onboarding: this window only hears the
    // broadcast and refreshes, so handleSignIn's reset never runs here.
    checkSession.mockResolvedValue({ token: 'session' });
    fireAuthChanged();
    await flushPromises();

    expect(wrapper.text()).toContain('Sign Out');
    expect(wrapper.find('.calendar-connect').exists()).toBe(false);
  });
});

describe('SettingsView backend switched elsewhere', () => {
  function fireBackendChanged() {
    const cb = listeners.get('backend://changed');
    expect(cb).toBeDefined();
    cb!({ payload: { source: 'library-x' } });
  }

  it('follows a switch to Local and asks before the first model download', async () => {
    hasPromptedLocalModels.mockResolvedValue(false);
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.get('.backend-trigger').text()).toContain('ariso.ai');

    getBackendSetting.mockResolvedValue('local' as never);
    fireBackendChanged();
    await flushPromises();

    expect(wrapper.get('.backend-trigger').text()).toContain('Local');
    expect(wrapper.find('.download-confirm').exists()).toBe(true);
    expect(downloadStt).not.toHaveBeenCalled();
    // Following a switch doesn't make one of its own.
    expect(setBackendSetting).not.toHaveBeenCalled();
    expect(emit).not.toHaveBeenCalledWith('backend://changed');
  });

  it('starts missing downloads right away when already prompted once', async () => {
    hasPromptedLocalModels.mockResolvedValue(true);
    const wrapper = mount(SettingsView);
    await flushPromises();

    getBackendSetting.mockResolvedValue('local' as never);
    fireBackendChanged();
    await flushPromises();

    expect(wrapper.find('.download-confirm').exists()).toBe(false);
    expect(downloadStt).toHaveBeenCalledTimes(1);
    expect(downloadLlm).toHaveBeenCalledTimes(1);
  });

  it('treats its own switch coming back as already applied', async () => {
    hasPromptedLocalModels.mockResolvedValue(true);
    const wrapper = mount(SettingsView);
    await flushPromises();
    await wrapper.get('.backend-trigger').trigger('click');
    await wrapper.findAll('.backend-option')[1].trigger('mousedown');
    await flushPromises();
    expect(downloadStt).toHaveBeenCalledTimes(1);

    getBackendSetting.mockResolvedValue('local' as never);
    fireBackendChanged();
    await flushPromises();

    expect(downloadStt).toHaveBeenCalledTimes(1);
  });

  it('drops the download confirmation when switched back to ariso.ai elsewhere', async () => {
    hasPromptedLocalModels.mockResolvedValue(false);
    const wrapper = mount(SettingsView);
    await flushPromises();
    getBackendSetting.mockResolvedValue('local' as never);
    fireBackendChanged();
    await flushPromises();
    expect(wrapper.find('.download-confirm').exists()).toBe(true);

    getBackendSetting.mockResolvedValue('ariso');
    fireBackendChanged();
    await flushPromises();

    expect(wrapper.find('.download-confirm').exists()).toBe(false);
    expect(wrapper.get('.backend-trigger').text()).toContain('ariso.ai');
  });
});

describe('SettingsView auth broadcast', () => {
  it('clears the account card when the session ends elsewhere', async () => {
    checkSession.mockResolvedValue({ sessionToken: 'session' });
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.text()).toContain('Sign Out');

    // Signed out from another window, or a rejected session cleared natively.
    checkSession.mockResolvedValue(null);
    fireAuthChanged();
    await flushPromises();

    expect(wrapper.text()).not.toContain('Sign Out');
    expect(wrapper.get('.google-btn').text()).toContain('Sign in with Google');
  });

  it('shows a sign-in from another window without a remount', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.find('.sign-in-container').exists()).toBe(true);

    checkSession.mockResolvedValue({ sessionToken: 'session' });
    fireAuthChanged();
    await flushPromises();

    expect(wrapper.text()).toContain('Sign Out');
    // The broadcast only prompts a re-read; this window starts no flow.
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(microsoftSignIn).not.toHaveBeenCalled();
  });

  it('keeps the recording sign-in banner until a session actually exists', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    listeners.get('tray://show-sign-in-prompt')!({ payload: null });
    await flushPromises();
    expect(wrapper.text()).toContain('Please sign in to start recording.');

    // A sign-out elsewhere is also a change, but leaves the user signed out.
    fireAuthChanged();
    await flushPromises();
    expect(wrapper.text()).toContain('Please sign in to start recording.');

    checkSession.mockResolvedValue({ sessionToken: 'session' });
    fireAuthChanged();
    await flushPromises();
    expect(wrapper.text()).not.toContain('Please sign in to start recording.');
  });
});

describe('SettingsView tray sign-in', () => {
  function fireTraySignIn(payload: unknown) {
    const cb = listeners.get('tray://sign-in');
    expect(cb).toBeDefined();
    cb!({ payload });
  }

  it('starts the requested provider flow like a button click', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    fireTraySignIn('microsoft');
    await flushPromises();
    expect(microsoftSignIn).toHaveBeenCalledTimes(1);
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('Sign Out');

    await wrapper.get('.account-info .sign-out-btn').trigger('click');
    await flushPromises();
    fireTraySignIn('google');
    await flushPromises();
    expect(googleSignIn).toHaveBeenCalledTimes(1);
    // Same post-sign-in path as the button: Google still runs the calendar hop.
    expect(ensureCalendarAccess).toHaveBeenCalledTimes(1);
  });

  it('shows the pending flow so it can be canceled from Settings', async () => {
    googleSignIn.mockReturnValue(new Promise(() => {}));
    const wrapper = mount(SettingsView);
    await flushPromises();

    fireTraySignIn('google');
    await flushPromises();

    expect(wrapper.get('.google-btn').text()).toContain('Continue in your browser…');
    expect(wrapper.find('.sign-in-cancel').exists()).toBe(true);
  });

  it('ignores a request while a flow is already pending', async () => {
    microsoftSignIn.mockReturnValue(new Promise(() => {}));
    mount(SettingsView);
    await flushPromises();

    // Two tray clicks landing back to back must not start two flows.
    fireTraySignIn('microsoft');
    fireTraySignIn('google');
    await flushPromises();
    fireTraySignIn('google');
    await flushPromises();

    expect(microsoftSignIn).toHaveBeenCalledTimes(1);
    expect(googleSignIn).not.toHaveBeenCalled();
  });

  it('does not sign in again when the session turns out to be valid', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    // Signed in elsewhere since this window last looked.
    checkSession.mockResolvedValue({ sessionToken: 'session' });
    fireTraySignIn('google');
    await flushPromises();

    expect(googleSignIn).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('Sign Out');
  });

  it('re-reads a stale signed-in state before starting the flow', async () => {
    checkSession.mockResolvedValue({ sessionToken: 'session' });
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(wrapper.text()).toContain('Sign Out');

    // The server rejected the session and native code cleared it without
    // telling this window; the tray offers sign-in again.
    checkSession.mockResolvedValue(null);
    fireTraySignIn('microsoft');
    await flushPromises();

    expect(microsoftSignIn).toHaveBeenCalledTimes(1);
  });

  it('never signs in on the Local backend', async () => {
    // Offline mode makes no network calls; a request that raced a switch to
    // Local must not start one.
    getBackendSetting.mockResolvedValue('local' as never);
    mount(SettingsView);
    await flushPromises();

    fireTraySignIn('google');
    await flushPromises();

    expect(checkSession).toHaveBeenCalledTimes(1); // mount only
    expect(googleSignIn).not.toHaveBeenCalled();
  });

  it('ignores an unknown provider', async () => {
    mount(SettingsView);
    await flushPromises();

    fireTraySignIn('github');
    await flushPromises();

    expect(googleSignIn).not.toHaveBeenCalled();
    expect(microsoftSignIn).not.toHaveBeenCalled();
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

describe('SettingsView auto-record depends on the microphone', () => {
  function autoRecordToggle(wrapper: ReturnType<typeof mount>) {
    const row = wrapper
      .findAll('.setting-row')
      .find((r) => r.find('.setting-label').text() === 'Auto-record meetings');
    expect(row, 'Auto-record meetings row').toBeDefined();
    return row!.find('input.toggle-input');
  }

  function micToggle(wrapper: ReturnType<typeof mount>) {
    const row = wrapper
      .findAll('.setting-row')
      .find((r) => r.find('.setting-label').text() === 'Microphone');
    return row!.find('input.toggle-input');
  }

  it('disables auto-record with a hint while the microphone is off', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(autoRecordToggle(wrapper).attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain('Turn on Microphone to detect meetings.');
  });

  it('enables auto-record when the microphone is on', async () => {
    loadRecordingEnabled.mockResolvedValue({ mic: true, systemAudio: false });
    const wrapper = mount(SettingsView);
    await flushPromises();
    expect(autoRecordToggle(wrapper).attributes('disabled')).toBeUndefined();
    expect(wrapper.text()).not.toContain('Turn on Microphone to detect meetings.');
  });

  it('re-enables auto-record as soon as the microphone is toggled on', async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();
    const mic = micToggle(wrapper);
    (mic.element as HTMLInputElement).checked = true;
    await mic.trigger('change');
    await flushPromises();
    expect(autoRecordToggle(wrapper).attributes('disabled')).toBeUndefined();
  });
});
describe('SettingsView language models table', () => {
  const ROW = '[data-test="model-row"]';
  const GEMMA = 'Gemma 3 1B';
  const PARAKEET = 'Parakeet TDT 0.6B v3';

  function rows(wrapper: ReturnType<typeof mount>) {
    return wrapper.findAll(ROW);
  }

  /** Rows are addressed by name: the table also lists the remote models, so a
   *  position says nothing about which model a row is. */
  function rowNamed(wrapper: ReturnType<typeof mount>, name: string) {
    const row = rows(wrapper).find((r) => r.text().includes(name));
    if (!row) throw new Error(`no model row named ${name}`);
    return row;
  }

  it('has no model table on the Ariso backend', async () => {
    getBackendSetting.mockResolvedValue('ariso');
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find(ROW).exists()).toBe(false);
  });

  it('names the section "AI Models"', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.text()).toContain('AI Models');
    expect(wrapper.text()).not.toContain('Language models');
    expect(wrapper.text()).not.toContain('On-device models');
    expect(wrapper.text()).not.toContain('Local models');
  });

  it('lists each model with its name, type and runtime', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    const gemma = rowNamed(wrapper, GEMMA);
    expect(gemma.get('[data-test="model-type-icon"]').attributes('aria-label')).toContain(
      'Language model',
    );
    // A local model is shown by its size on disk; only remote rows are labelled.
    expect(gemma.find('[data-test="model-size"]').exists()).toBe(true);
    const parakeet = rowNamed(wrapper, PARAKEET);
    expect(parakeet.get('[data-test="model-type-icon"]').attributes('aria-label')).toContain(
      'Speech model',
    );
  });

  it('ticks the in-use model of each type', async () => {
    getBackendSetting.mockResolvedValue('local');
    // Only an installed model can be in use, so the tick needs both downloaded.
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    const wrapper = mount(SettingsView);
    await flushPromises();

    // One notes model writes notes, one speech model transcribes — both in use.
    expect(rowNamed(wrapper, GEMMA).find('.model-tick--on').exists()).toBe(true);
    expect(rowNamed(wrapper, PARAKEET).find('.model-tick--on').exists()).toBe(true);
  });

  it('selects an installed notes model when its row is clicked', async () => {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, GEMMA).trigger('click');
    await flushPromises();

    expect(setNotesModelSetting).toHaveBeenCalledWith({
      kind: 'local',
      id: 'gemma-3-1b-it-qat-4bit',
    });
  });

  it('cannot put a model that is not downloaded into use', async () => {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'not_downloaded', llmReady: false });
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, GEMMA).trigger('click');
    await flushPromises();

    // Nothing on disk to run, so the row cannot claim the tick.
    expect(setNotesModelSetting).not.toHaveBeenCalled();
    expect(rowNamed(wrapper, GEMMA).find('.model-tick--on').exists()).toBe(false);
  });

  it('does not touch the notes selection when a speech row is clicked', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rows(wrapper)[1].trigger('click');
    await flushPromises();

    expect(setNotesModelSetting).not.toHaveBeenCalled();
  });

  it('shows the model type as an icon, with details on hover', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    const icon = rowNamed(wrapper, GEMMA).get('[data-test="model-type-icon"]');
    expect(icon.find('svg').exists()).toBe(true);
    expect(icon.attributes('aria-label')).toContain('Language model');
    // Same styled bubble the vault "?" uses, revealed on hover/focus.
    expect(rowNamed(wrapper, GEMMA).get('[data-test="model-details"]').text()).toContain(
      'gemma-3-1b-it-qat-4bit',
    );
    const speechIcon = rowNamed(wrapper, PARAKEET).get('[data-test="model-type-icon"]');
    expect(speechIcon.attributes('aria-label')).toContain('Speech model');
    expect(rowNamed(wrapper, PARAKEET).get('[data-test="model-details"]').text()).toContain(
      'parakeet-tdt-0.6b-v3',
    );
  });

  it('installs a model from its own row', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, GEMMA).get('[data-test="install-model"]').trigger('click');
    await flushPromises();
    expect(downloadLlm).toHaveBeenCalledTimes(1);

    await rowNamed(wrapper, PARAKEET).get('[data-test="install-model"]').trigger('click');
    await flushPromises();
    expect(downloadStt).toHaveBeenCalledTimes(1);
  });

  it('installing does not also change the notes selection', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, PARAKEET).get('[data-test="install-model"]').trigger('click');
    await flushPromises();

    expect(setNotesModelSetting).not.toHaveBeenCalled();
  });

  it('drops the install button once a model is installed', async () => {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(rowNamed(wrapper, GEMMA).find('[data-test="install-model"]').exists()).toBe(false);
    expect(rowNamed(wrapper, PARAKEET).find('[data-test="install-model"]').exists()).toBe(false);
    expect(rowNamed(wrapper, GEMMA).find('[data-test="remove-model"]').exists()).toBe(true);
  });

  it('shows a dash for a size the backend cannot report yet', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(rowNamed(wrapper, GEMMA).get('[data-test="model-size"]').text()).toBe('—');
  });

  it('scrolls the list rather than growing past six rows', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    // More models ship than fit, so the list is a scroll box sized in rows.
    expect(rows(wrapper).length).toBeGreaterThan(6);
    const scroller = wrapper.get('[data-test="model-scroll"]');
    expect(scroller.attributes('style')).toContain('--model-visible-rows: 6');
    expect(scroller.find('.model-table').exists()).toBe(true);
  });

  it('keeps the vault out of the models section', async () => {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();

    const models = wrapper.get('[data-test="models-section"]');
    const vault = wrapper.get('[data-test="vault-section"]');
    expect(models.find('[data-test="vault-path"]').exists()).toBe(false);
    expect(vault.find('[data-test="vault-path"]').exists()).toBe(true);
  });
});

describe('SettingsView remote model API keys', () => {
  const ROW = '[data-test="model-row"]';
  const GEMMA = 'Gemma 3 1B';

  function rowNamed(wrapper: ReturnType<typeof mount>, name: string) {
    const row = wrapper.findAll(ROW).find((r) => r.text().includes(name));
    if (!row) throw new Error(`no model row named ${name}`);
    return row;
  }

  async function mountLocal() {
    getBackendSetting.mockResolvedValue('local');
    const wrapper = mount(SettingsView);
    await flushPromises();
    return wrapper;
  }

  /** Walk a provider's row from "Connect" through a saved key. */
  async function connect(
    wrapper: ReturnType<typeof mount>,
    name: string,
    key: string,
  ) {
    await rowNamed(wrapper, name).get('[data-test="connect-key"]').trigger('click');
    const input = wrapper.get('[data-test="api-key-input"]');
    await input.setValue(key);
    await wrapper.get('[data-test="save-key"]').trigger('click');
    await flushPromises();
  }

  it('lists the remote models alongside the on-device ones', async () => {
    const wrapper = await mountLocal();

    for (const name of ['Claude Haiku 4.5', 'GPT-5.1', 'Gemini 3.7 Flash']) {
      expect(rowNamed(wrapper, name).find('[data-test="connect-key"]').exists()).toBe(true);
    }
  });

  it('marks a model as remote with its type icon, not a word in the row', async () => {
    const wrapper = await mountLocal();

    const remote = rowNamed(wrapper, 'Claude Haiku 4.5');
    expect(remote.text()).not.toContain('Remote');
    expect(remote.get('[data-test="model-type-icon"]').attributes('aria-label')).toContain(
      'Remote language model',
    );
    // The on-device rows keep the icon they had.
    expect(rowNamed(wrapper, GEMMA).get('[data-test="model-type-icon"]').attributes('aria-label'))
      .toContain('Language model');
  });

  it('offers to connect a provider that has no key stored', async () => {
    const wrapper = await mountLocal();

    // Icon only, like Install and Delete: the word is the hover tip.
    const row = rowNamed(wrapper, 'Claude Haiku 4.5');
    const connectBtn = row.get('[data-test="connect-key"]');
    expect(connectBtn.text()).toBe('');
    expect(connectBtn.find('svg').exists()).toBe(true);
    expect(connectBtn.attributes('title')).toBe('Connect');
    expect(connectBtn.attributes('aria-label')).toBe('Connect Anthropic');
    expect(row.find('[data-test="remove-key"]').exists()).toBe(false);
  });

  it('asks for the key in a masked field, never a plain one', async () => {
    const wrapper = await mountLocal();

    await rowNamed(wrapper, 'Claude Haiku 4.5').get('[data-test="connect-key"]').trigger('click');

    const input = wrapper.get('[data-test="api-key-input"]');
    expect(input.attributes('type')).toBe('password');
  });

  it('discloses where the transcript goes before a key is entered', async () => {
    const wrapper = await mountLocal();

    await rowNamed(wrapper, 'Claude Haiku 4.5').get('[data-test="connect-key"]').trigger('click');

    const disclosure = wrapper.get('[data-test="remote-disclosure"]').text();
    expect(disclosure).toContain('Anthropic');
    expect(disclosure).toContain('stay on this device');
  });

  it('stores an entered key against its provider', async () => {
    const wrapper = await mountLocal();

    await connect(wrapper, 'Claude Haiku 4.5', 'sk-ant-test');

    expect(setLlmApiKey).toHaveBeenCalledWith('anthropic', 'sk-ant-test');
  });

  it('shows the provider as connected once its key is saved', async () => {
    const wrapper = await mountLocal();

    await connect(wrapper, 'Claude Haiku 4.5', 'sk-ant-test');

    const row = rowNamed(wrapper, 'Claude Haiku 4.5');
    // The swapped icon is the whole signal — no "Connected" word in the row.
    expect(row.find('[data-test="connect-key"]').exists()).toBe(false);
    const disconnect = row.get('[data-test="remove-key"]');
    expect(disconnect.attributes('title')).toBe('Disconnect');
    expect(disconnect.text()).toBe('');
    expect(row.text()).not.toContain('Connected');
    expect(wrapper.find('[data-test="api-key-input"]').exists()).toBe(false);
  });

  it('connects every model of a provider from one key', async () => {
    const wrapper = await mountLocal();

    await connect(wrapper, 'Claude Haiku 4.5', 'sk-ant-test');

    // One keychain entry per provider, not per model.
    expect(rowNamed(wrapper, 'Claude Sonnet 5').find('[data-test="remove-key"]').exists()).toBe(
      true,
    );
    expect(rowNamed(wrapper, 'GPT-5.1').find('[data-test="remove-key"]').exists()).toBe(false);
  });

  it('shows a provider connected on a previous run without asking again', async () => {
    llmApiKeyProviders.mockResolvedValue(['openai']);
    const wrapper = await mountLocal();

    expect(rowNamed(wrapper, 'GPT-5.1').find('[data-test="remove-key"]').exists()).toBe(true);
    expect(rowNamed(wrapper, 'Claude Haiku 4.5').find('[data-test="remove-key"]').exists()).toBe(
      false,
    );
  });

  it('forgets a key when the provider is disconnected', async () => {
    llmApiKeyProviders.mockResolvedValue(['anthropic']);
    const wrapper = await mountLocal();

    await rowNamed(wrapper, 'Claude Haiku 4.5').get('[data-test="remove-key"]').trigger('click');
    await flushPromises();

    expect(clearLlmApiKey).toHaveBeenCalledWith('anthropic');
    expect(rowNamed(wrapper, 'Claude Haiku 4.5').find('[data-test="connect-key"]').exists()).toBe(
      true,
    );
  });

  it('keeps the field open and says why when the backend rejects a key', async () => {
    setLlmApiKey.mockRejectedValue(new Error('That API key contains characters a key can’t hold.'));
    const wrapper = await mountLocal();

    await connect(wrapper, 'Claude Haiku 4.5', 'bad key');

    expect(wrapper.get('[data-test="key-error"]').text()).toContain('characters');
    expect(wrapper.find('[data-test="api-key-input"]').exists()).toBe(true);
    expect(rowNamed(wrapper, 'Claude Haiku 4.5').find('[data-test="remove-key"]').exists()).toBe(
      false,
    );
  });

  it('makes a connected remote model the one in use when its row is clicked', async () => {
    llmApiKeyProviders.mockResolvedValue(['anthropic']);
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    const wrapper = await mountLocal();

    await rowNamed(wrapper, 'Claude Haiku 4.5').trigger('click');
    await flushPromises();

    expect(setNotesModelSetting).toHaveBeenCalledWith({
      kind: 'remote',
      provider: 'anthropic',
      id: 'claude-haiku-4-5',
    });
    expect(rowNamed(wrapper, 'Claude Haiku 4.5').find('.model-tick--on').exists()).toBe(true);
    // One notes model at a time: the on-device row gives the tick up.
    expect(rowNamed(wrapper, 'Gemma 3 1B').find('.model-tick--on').exists()).toBe(false);
  });

  it('asks for a key instead of selecting a provider that has none', async () => {
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    const wrapper = await mountLocal();

    await rowNamed(wrapper, 'Claude Haiku 4.5').trigger('click');
    await flushPromises();

    // Nothing to call the model with yet, so the click opens the key field.
    expect(setNotesModelSetting).not.toHaveBeenCalled();
    expect(wrapper.find('[data-test="api-key-input"]').exists()).toBe(true);
    expect(rowNamed(wrapper, 'Gemma 3 1B').find('.model-tick--on').exists()).toBe(true);
  });

  it('selects a remote model as soon as its key is saved and its row clicked', async () => {
    const wrapper = await mountLocal();

    await connect(wrapper, 'Claude Haiku 4.5', 'sk-ant-test');
    await rowNamed(wrapper, 'Claude Haiku 4.5').trigger('click');
    await flushPromises();

    expect(setNotesModelSetting).toHaveBeenCalledWith({
      kind: 'remote',
      provider: 'anthropic',
      id: 'claude-haiku-4-5',
    });
  });

  it('says a remote model in use is not generating notes yet', async () => {
    llmApiKeyProviders.mockResolvedValue(['anthropic']);
    getNotesModelSetting.mockResolvedValue({
      kind: 'remote',
      provider: 'anthropic',
      id: 'claude-haiku-4-5',
    } as never);
    const wrapper = await mountLocal();

    expect(wrapper.get('[data-test="models-section"]').text()).toContain(
      'Notes are still written on this device',
    );
  });

  it('keeps that caveat out of the way while notes stay on-device', async () => {
    llmApiKeyProviders.mockResolvedValue(['anthropic']);
    const wrapper = await mountLocal();

    // A connected provider that is not the model in use says nothing.
    expect(wrapper.find('[data-test="remote-pending"]').exists()).toBe(false);
  });

  it('leaves the remote rows out of the Ariso backend entirely', async () => {
    getBackendSetting.mockResolvedValue('ariso');
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.find('[data-test="connect-key"]').exists()).toBe(false);
    expect(llmApiKeyProviders).not.toHaveBeenCalled();
  });
});

describe('SettingsView model size and removal', () => {
  const ROW = '[data-test="model-row"]';
  const MB = 1024 * 1024;
  const GEMMA = 'Gemma 3 1B';
  const PARAKEET = 'Parakeet TDT 0.6B v3';

  function rows(wrapper: ReturnType<typeof mount>) {
    return wrapper.findAll(ROW);
  }

  function rowNamed(wrapper: ReturnType<typeof mount>, name: string) {
    const row = rows(wrapper).find((r) => r.text().includes(name));
    if (!row) throw new Error(`no model row named ${name}`);
    return row;
  }

  async function mountInstalled() {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
    modelSizes.mockResolvedValue({ notes: 750 * MB, speech: 900 * MB });
    const wrapper = mount(SettingsView);
    await flushPromises();
    return wrapper;
  }

  it('shows each model size on disk', async () => {
    const wrapper = await mountInstalled();

    expect(rowNamed(wrapper, GEMMA).get('[data-test="model-size"]').text()).toBe('750 MB');
    expect(rowNamed(wrapper, PARAKEET).get('[data-test="model-size"]').text()).toBe('900 MB');
  });

  it('offers a Delete icon for an installed model and a download icon for a missing one', async () => {
    const installed = await mountInstalled();
    const remove = rowNamed(installed, GEMMA).get('[data-test="remove-model"]');
    expect(remove.attributes('aria-label')).toBe('Delete');
    expect(remove.attributes('title')).toBe('Delete');
    expect(remove.text()).toBe('');
    expect(remove.find('svg').exists()).toBe(true);
    expect(installed.find('[data-test="install-model"]').exists()).toBe(false);

    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'not_downloaded', llmReady: false });
    const missing = mount(SettingsView);
    await flushPromises();
    expect(missing.find('[data-test="remove-model"]').exists()).toBe(false);

    // Icon only: "Install" is the hover tip, never a label in the row.
    const install = rowNamed(missing, GEMMA).get('[data-test="install-model"]');
    expect(install.text()).toBe('');
    expect(install.find('svg').exists()).toBe(true);
    expect(install.attributes('title')).toBe('Install');
    expect(install.attributes('aria-label')).toBe('Install');
    expect(missing.text()).not.toContain('Install');
  });

  it('calls the tip Downloading while the model downloads', async () => {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'not_downloaded', llmReady: false });
    // Never settles, so the row stays in its mid-download state.
    downloadStt.mockReturnValue(new Promise(() => {}));
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, PARAKEET).get('[data-test="install-model"]').trigger('click');
    await flushPromises();

    const install = rowNamed(wrapper, PARAKEET).get('[data-test="install-model"]');
    expect(install.attributes('title')).toBe('Downloading');
    expect(install.attributes('aria-label')).toBe('Downloading');
    expect(install.text()).toBe('');

    // The size is only ever the "—" placeholder mid-download, so it is dropped
    // and the progress text takes the space.
    expect(rowNamed(wrapper, PARAKEET).find('[data-test="model-size"]').exists()).toBe(false);
    expect(rowNamed(wrapper, PARAKEET).text()).not.toContain('—');
    // The row that is not downloading keeps its size.
    expect(rowNamed(wrapper, GEMMA).find('[data-test="model-size"]').exists()).toBe(true);
  });

  it('yields the size to a failure message rather than showing both', async () => {
    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'not_downloaded', llmReady: false });
    modelSizes.mockResolvedValue({ notes: 750 * MB, speech: 900 * MB });
    downloadStt.mockRejectedValue(new Error('network'));
    const wrapper = mount(SettingsView);
    await flushPromises();

    await rowNamed(wrapper, PARAKEET).get('[data-test="install-model"]').trigger('click');
    await flushPromises();

    // The column is a fixed width, so a size beside this message would squeeze
    // it into an unreadable ribbon; the status gets the space instead.
    expect(rowNamed(wrapper, PARAKEET).text()).toContain('Download failed');
    expect(rowNamed(wrapper, PARAKEET).find('[data-test="model-size"]').exists()).toBe(false);
  });

  it('drops the in-use tick when the selected model is not installed', async () => {
    const installed = await mountInstalled();
    expect(installed.findAll('.model-tick--on').length).toBe(2);

    getBackendSetting.mockResolvedValue('local');
    modelStatus.mockResolvedValue({ state: 'not_downloaded', llmReady: false });
    const missing = mount(SettingsView);
    await flushPromises();

    // No other model of either type ships today, so no row claims the tick.
    expect(missing.find('.model-tick--on').exists()).toBe(false);
  });

  it('asks before removing a model', async () => {
    const wrapper = await mountInstalled();

    await rowNamed(wrapper, GEMMA).get('[data-test="remove-model"]').trigger('click');
    await flushPromises();

    expect(wrapper.find('[data-test="remove-confirm"]').exists()).toBe(true);
    expect(deleteModel).not.toHaveBeenCalled();
  });

  it('removes the files once confirmed', async () => {
    const wrapper = await mountInstalled();
    await rowNamed(wrapper, GEMMA).get('[data-test="remove-model"]').trigger('click');
    await flushPromises();

    await wrapper.get('[data-test="remove-confirm-ok"]').trigger('click');
    await flushPromises();

    expect(deleteModel).toHaveBeenCalledWith('notes');
  });

  it('names the speech model kind when removing that row', async () => {
    const wrapper = await mountInstalled();
    await rowNamed(wrapper, PARAKEET).get('[data-test="remove-model"]').trigger('click');
    await flushPromises();
    await wrapper.get('[data-test="remove-confirm-ok"]').trigger('click');
    await flushPromises();

    expect(deleteModel).toHaveBeenCalledWith('speech');
  });

  it('keeps the model when the confirm is dismissed', async () => {
    const wrapper = await mountInstalled();
    await rows(wrapper)[0].get('[data-test="remove-model"]').trigger('click');
    await flushPromises();

    await wrapper.get('[data-test="remove-confirm-cancel"]').trigger('click');
    await flushPromises();

    expect(deleteModel).not.toHaveBeenCalled();
    expect(wrapper.find('[data-test="remove-confirm"]').exists()).toBe(false);
  });

  it('re-reads status and sizes after a removal', async () => {
    const wrapper = await mountInstalled();
    modelStatus.mockClear();
    modelSizes.mockClear();

    await rows(wrapper)[0].get('[data-test="remove-model"]').trigger('click');
    await flushPromises();
    await wrapper.get('[data-test="remove-confirm-ok"]').trigger('click');
    await flushPromises();

    expect(modelStatus).toHaveBeenCalled();
    expect(modelSizes).toHaveBeenCalled();
  });

  it('surfaces a refused removal instead of failing silently', async () => {
    const wrapper = await mountInstalled();
    deleteModel.mockRejectedValue(new Error('Can\'t remove a model while a recording is in progress.'));

    await rows(wrapper)[0].get('[data-test="remove-model"]').trigger('click');
    await flushPromises();
    await wrapper.get('[data-test="remove-confirm-ok"]').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('Can\'t remove a model while a recording is in progress.');
  });
});
