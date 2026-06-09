// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const googleSignIn = vi.fn();
const setOnboarded = vi.fn();
const emitNotificationsSync = vi.fn();
const close = vi.fn();

vi.mock('../tauri', () => ({
  auth: { googleSignIn: () => googleSignIn() },
  setOnboarded: (v: boolean) => setOnboarded(v),
}));
vi.mock('../composables/useMeetingNotifications', () => ({
  emitNotificationsSync: () => emitNotificationsSync(),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close: () => close() }),
}));

import OnboardingView from './OnboardingView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  setOnboarded.mockResolvedValue(undefined);
  emitNotificationsSync.mockResolvedValue(undefined);
});

describe('OnboardingView', () => {
  it('renders the sign-in step with Google and Skip buttons', () => {
    const wrapper = mount(OnboardingView);
    expect(wrapper.find('.google-btn').exists()).toBe(true);
    expect(wrapper.find('.skip-btn').exists()).toBe(true);
  });

  it('Skip finishes the single-step flow: sets the flag and closes the window', async () => {
    const wrapper = mount(OnboardingView);
    await wrapper.find('.skip-btn').trigger('click');
    await flushPromises();
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(setOnboarded).toHaveBeenCalledWith(true);
    expect(close).toHaveBeenCalled();
  });

  it('successful sign-in syncs notifications, sets the flag, and closes', async () => {
    googleSignIn.mockResolvedValue({ success: true, sessionToken: 't' });
    const wrapper = mount(OnboardingView);
    await wrapper.find('.google-btn').trigger('click');
    await flushPromises();
    expect(emitNotificationsSync).toHaveBeenCalled();
    expect(setOnboarded).toHaveBeenCalledWith(true);
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
