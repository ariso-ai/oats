// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const close = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => ({ close }) }));

import MeetingEndPromptView from './MeetingEndPromptView.vue';

function byText(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text);
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = '#/meeting-end-prompt?seconds=30&subtitle=Weekly%20sync';
});

describe('MeetingEndPromptView', () => {
  it('renders the fixed title and the meeting subtitle', () => {
    const wrapper = mount(MeetingEndPromptView);
    expect(wrapper.text()).toContain('Meeting ended');
    expect(wrapper.find('[data-test="subtitle"]').text()).toBe('Weekly sync');
  });

  it('hides the subtitle line when none is provided', () => {
    window.location.hash = '#/meeting-end-prompt?seconds=30';
    const wrapper = mount(MeetingEndPromptView);
    expect(wrapper.find('[data-test="subtitle"]').exists()).toBe(false);
  });

  it('drives the countdown bar from the seconds param', () => {
    const wrapper = mount(MeetingEndPromptView);
    const bar = wrapper.find('[data-test="countdown-fill"]');
    expect((bar.element as HTMLElement).style.animationDuration).toBe('30s');
  });

  it('resolves with stop=true when Stop is clicked', async () => {
    const wrapper = mount(MeetingEndPromptView);
    await byText(wrapper, 'Stop')!.trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_end_prompt', { stop: true });
  });

  it('resolves with stop=false and closes from the corner dismiss', async () => {
    const wrapper = mount(MeetingEndPromptView);
    await wrapper.find('[data-test="dismiss"]').trigger('click');
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith('resolve_meeting_end_prompt', { stop: false });
    expect(close).toHaveBeenCalled();
  });

  it('toggles the more-options menu and grows/shrinks the window', async () => {
    const wrapper = mount(MeetingEndPromptView);
    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_meeting_end_prompt', { expanded: true });
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(true);
    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_meeting_end_prompt', { expanded: false });
  });
});
