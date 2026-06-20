// @vitest-environment jsdom
// src/views/SilencePromptView.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const close = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => ({ close }) }));

import SilencePromptView from './SilencePromptView.vue';

function byText(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text);
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = '#/silence-prompt?seconds=60&subtitle=Weekly%20sync';
});

describe('SilencePromptView', () => {
  it('renders the fixed title and the meeting subtitle', () => {
    const wrapper = mount(SilencePromptView);
    expect(wrapper.text()).toContain('Meeting is silent');
    expect(wrapper.find('[data-test="subtitle"]').text()).toBe('Weekly sync');
  });

  it('hides the subtitle line when no subtitle is provided', () => {
    window.location.hash = '#/silence-prompt?seconds=60';
    const wrapper = mount(SilencePromptView);
    expect(wrapper.text()).toContain('Meeting is silent');
    expect(wrapper.find('[data-test="subtitle"]').exists()).toBe(false);
  });

  it('drives the countdown bar from the seconds param', () => {
    const wrapper = mount(SilencePromptView);
    const bar = wrapper.find('[data-test="countdown-fill"]');
    expect(bar.exists()).toBe(true);
    expect((bar.element as HTMLElement).style.animationDuration).toBe('60s');
  });

  it('resolves with stop=true when Stop recording is clicked', async () => {
    const wrapper = mount(SilencePromptView);
    await byText(wrapper, 'Stop recording')!.trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_silence_prompt', { stop: true });
  });

  it('resolves with stop=false and closes the banner from the corner dismiss button', async () => {
    const wrapper = mount(SilencePromptView);
    await wrapper.find('[data-test="dismiss"]').trigger('click');
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith('resolve_silence_prompt', { stop: false });
    expect(close).toHaveBeenCalled();
  });

  it('toggles the more-options menu and grows/shrinks the window', async () => {
    const wrapper = mount(SilencePromptView);
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(false);

    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_silence_prompt', { expanded: true });
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(true);

    await wrapper.find('[data-test="more-options"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resize_silence_prompt', { expanded: false });
    expect(wrapper.find('[data-test="menu-dismiss"]').exists()).toBe(false);
  });

  it('resolves with stop=false from the menu Dismiss item (keep recording)', async () => {
    const wrapper = mount(SilencePromptView);
    await wrapper.find('[data-test="more-options"]').trigger('click');
    await wrapper.find('[data-test="menu-dismiss"]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('resolve_silence_prompt', { stop: false });
  });
});
