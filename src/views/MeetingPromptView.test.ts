// @vitest-environment jsdom
// src/views/MeetingPromptView.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const close = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => ({ close }) }));

import MeetingPromptView from './MeetingPromptView.vue';

function byText(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text);
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = '#/meeting-prompt?seconds=8';
});

describe('MeetingPromptView', () => {
  it('renders the default title and subtitle', () => {
    const wrapper = mount(MeetingPromptView);
    expect(wrapper.text()).toContain('Meeting started');
    expect(wrapper.text()).toContain('oats can take notes for you.');
  });

  it('drives the countdown bar from the seconds param', () => {
    const wrapper = mount(MeetingPromptView);
    const bar = wrapper.find('[data-test="countdown-fill"]');
    expect(bar.exists()).toBe(true);
    expect((bar.element as HTMLElement).style.animationDuration).toBe('8s');
  });

  it('resolves with record=true when Take notes is clicked', async () => {
    const wrapper = mount(MeetingPromptView);
    await byText(wrapper, 'Take notes')!.trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_prompt', { record: true });
  });

  it('resolves with record=false and closes the banner from the dismiss button', async () => {
    const wrapper = mount(MeetingPromptView);
    await wrapper.find('[data-test="dismiss"]').trigger('click');
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_prompt', { record: false });
    expect(close).toHaveBeenCalled();
  });

  it('toggles the more-options menu and grows/shrinks the window', async () => {
    const wrapper = mount(MeetingPromptView);
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(false);

    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_meeting_prompt', { expanded: true });
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(true);

    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_meeting_prompt', { expanded: false });
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(false);
  });

  it('resolves with record=false from the menu Dismiss item', async () => {
    const wrapper = mount(MeetingPromptView);
    await wrapper.find('[data-test="more-options"]').trigger('click');
    await wrapper.find('[data-test="menu-dismiss"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_prompt', { record: false });
  });
});
