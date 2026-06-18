// @vitest-environment jsdom
// src/views/MeetingPromptView.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

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

  it('reveals Dismiss behind the chevron and resolves with record=false', async () => {
    const wrapper = mount(MeetingPromptView);
    expect(byText(wrapper, 'Dismiss')).toBeFalsy(); // hidden until the chevron is opened
    await wrapper.find('[data-test="disclosure"]').trigger('click');
    await byText(wrapper, 'Dismiss')!.trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_prompt', { record: false });
  });
});
