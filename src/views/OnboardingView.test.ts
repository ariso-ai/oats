// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const googleSignIn = vi.fn();
const cancelSignIn = vi.fn();
const setOnboarded = vi.fn();
const openSettingsWindow = vi.fn();
const emitNotificationsSync = vi.fn();
const emit = vi.fn();
const close = vi.fn();

vi.mock('../tauri', () => ({
  AUTH_SIGNED_IN_EVENT: 'auth://signed-in',
  SIGN_IN_CANCELED_ERROR: 'Sign-in canceled',
  auth: { googleSignIn: () => googleSignIn(), cancelSignIn: () => cancelSignIn() },
  openSettingsWindow: () => openSettingsWindow(),
  setOnboarded: (v: boolean) => setOnboarded(v),
}));
vi.mock('../composables/useMeetingNotifications', () => ({
  emitNotificationsSync: () => emitNotificationsSync(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: (event: string) => emit(event),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close: () => close() }),
}));

import OnboardingView from './OnboardingView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  setOnboarded.mockResolvedValue(undefined);
  openSettingsWindow.mockResolvedValue(undefined);
  emitNotificationsSync.mockResolvedValue(undefined);
  emit.mockResolvedValue(undefined);
});

describe('OnboardingView', () => {
  it('renders the sign-in step with Google and Skip buttons', () => {
    const wrapper = mount(OnboardingView);
    expect(wrapper.find('.google-btn').exists()).toBe(true);
    expect(wrapper.find('.skip-btn').exists()).toBe(true);
  });

  it('Skip opens settings, sets the flag, and closes the window', async () => {
    const wrapper = mount(OnboardingView);
    await wrapper.find('.skip-btn').trigger('click');
    await flushPromises();
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(setOnboarded).toHaveBeenCalledWith(true);
    expect(openSettingsWindow).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
  });

  it('replaces Skip with Cancel while browser sign-in is pending', async () => {
    let resolveSignIn!: (value: { success: boolean; sessionToken: string }) => void;
    googleSignIn.mockReturnValue(new Promise((resolve) => {
      resolveSignIn = resolve;
    }));
    const wrapper = mount(OnboardingView);
    await wrapper.find('.google-btn').trigger('click');
    await flushPromises();
    // Skip is replaced by a cancel affordance; clicking it must not finish
    // onboarding — it asks the backend to abort the pending flow.
    expect(wrapper.text()).toContain('Continue in your browser');
    const cancel = wrapper.find('.cancel-btn');
    expect(cancel.exists()).toBe(true);
    await cancel.trigger('click');
    expect(cancelSignIn).toHaveBeenCalled();
    expect(setOnboarded).not.toHaveBeenCalled();
    resolveSignIn({ success: true, sessionToken: 't' });
    await flushPromises();
    expect(setOnboarded).toHaveBeenCalledWith(true);
  });

  it('treats a canceled sign-in silently and restores the step', async () => {
    googleSignIn.mockResolvedValue({ error: 'Sign-in canceled' });
    const wrapper = mount(OnboardingView);
    await wrapper.find('.google-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.error').exists()).toBe(false);
    expect(wrapper.find('.cancel-btn').exists()).toBe(false);
    expect(setOnboarded).not.toHaveBeenCalled();
  });

  it('shows a completion error when Skip cannot finish onboarding', async () => {
    setOnboarded.mockRejectedValue(new Error('settings store failed'));
    const wrapper = mount(OnboardingView);
    await wrapper.find('.skip-btn').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('settings store failed');
    expect(openSettingsWindow).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
  });

  it('successful sign-in syncs notifications, opens settings, sets the flag, and closes', async () => {
    googleSignIn.mockResolvedValue({ success: true, sessionToken: 't' });
    const wrapper = mount(OnboardingView);
    await wrapper.find('.google-btn').trigger('click');
    await flushPromises();
    expect(emitNotificationsSync).toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith('auth://signed-in');
    expect(setOnboarded).toHaveBeenCalledWith(true);
    expect(openSettingsWindow).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
  });

  it('sign-in error stays on the step and shows the message', async () => {
    googleSignIn.mockResolvedValue({ error: 'boom' });
    const wrapper = mount(OnboardingView);
    await wrapper.find('.google-btn').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('boom');
    expect(setOnboarded).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
    expect(wrapper.find('.google-btn').exists()).toBe(true);
  });
});
