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
    checkSession: () => Promise.resolve(null),
    googleSignIn: vi.fn(),
    signOut: vi.fn(),
  },
  api: { request: vi.fn(() => Promise.resolve({ status: 200, data: {} })) },
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

import SettingsView from './SettingsView.vue';

// Remove each component's window 'focus' listener between tests.
enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  getAllWebviewWindows.mockResolvedValue([]);
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
