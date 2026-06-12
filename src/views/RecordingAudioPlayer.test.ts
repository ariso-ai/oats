// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import RecordingAudioPlayer from './RecordingAudioPlayer.vue';

beforeEach(() => {
  vi.restoreAllMocks();
  // jsdom lacks createObjectURL and media playback.
  URL.createObjectURL = vi.fn(() => 'blob:test');
  URL.revokeObjectURL = vi.fn();
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined);
});

describe('RecordingAudioPlayer', () => {
  it('loads bytes on Play and swaps to a native audio element', async () => {
    const load = vi.fn().mockResolvedValue(new ArrayBuffer(4));
    const wrapper = mount(RecordingAudioPlayer, { props: { load } });
    expect(wrapper.find('button.play-btn').text()).toContain('Play');

    await wrapper.find('button.play-btn').trigger('click');
    await flushPromises();

    expect(load).toHaveBeenCalledTimes(1);
    expect(wrapper.find('audio').exists()).toBe(true);
    expect(wrapper.find('audio').attributes('src')).toBe('blob:test');
  });

  it('shows a disabled "No audio" state when load resolves null', async () => {
    const load = vi.fn().mockResolvedValue(null);
    const wrapper = mount(RecordingAudioPlayer, { props: { load } });
    await wrapper.find('button.play-btn').trigger('click');
    await flushPromises();

    expect(wrapper.find('audio').exists()).toBe(false);
    expect(wrapper.find('button.play-btn').text()).toContain('No audio');
    expect(wrapper.find('button.play-btn').attributes('disabled')).toBeDefined();
  });

  it('shows Failed when load rejects', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const load = vi.fn().mockRejectedValue(new Error('500: audio fetch failed'));
    const wrapper = mount(RecordingAudioPlayer, { props: { load } });
    await wrapper.find('button.play-btn').trigger('click');
    await flushPromises();

    expect(wrapper.find('button.play-btn').text()).toContain('Failed');
    // Failed is retryable: button stays enabled.
    expect(wrapper.find('button.play-btn').attributes('disabled')).toBeUndefined();
  });
});
