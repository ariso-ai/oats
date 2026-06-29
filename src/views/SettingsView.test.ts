// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));
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
}));
vi.mock('../tauri', () => ({
  AUTH_SIGNED_IN_EVENT: 'auth://signed-in',
  auth: {
    checkSession: () => checkSession(),
    googleSignIn: vi.fn(),
    signOut: vi.fn(),
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
  emitNotificationsSync: vi.fn(() => Promise.resolve()),
}));
vi.mock('../composables/useAutoRecord', () => ({
  isAutoRecordEnabled: () => Promise.resolve(false),
  setAutoRecordEnabled: vi.fn(),
  isAutoRecordSupported: () => Promise.resolve(true),
}));
vi.mock('../composables/usePlatformCapabilities', () => ({
  defaultPlatformCapabilities: () => ({
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
  }),
  loadPlatformCapabilities: () => Promise.resolve({
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
  }),
}));
const setSilenceDetectionEnabled = vi.fn(() => Promise.resolve());
vi.mock('../composables/useSilenceDetection', () => ({
  isSilenceDetectionEnabled: () => Promise.resolve(true),
  setSilenceDetectionEnabled: (...a: unknown[]) => setSilenceDetectionEnabled(...a),
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

  // Sign the user in and route the two profile calls fetchUserProfile makes.
  function mockSignedIn(avatar: string | null) {
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
      return Promise.resolve({ status: 200, data: {} });
    });
  }

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
