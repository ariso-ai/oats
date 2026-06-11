// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

// In-test event bus standing in for Tauri's app-wide events.
type EventHandler = (e: { payload: unknown }) => void;
const eventHandlers = new Map<string, EventHandler[]>();
const emitEvent = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: EventHandler) => {
    const arr = eventHandlers.get(name) ?? [];
    arr.push(cb);
    eventHandlers.set(name, arr);
    return Promise.resolve(() => {});
  },
  emit: (...a: unknown[]) => emitEvent(...a),
}));

import RecorderStrip from './RecorderStrip.vue';

function sendState(payload: Record<string, unknown>): void {
  for (const cb of eventHandlers.get('recorder://state') ?? []) cb({ payload });
}

const recording = (over: Record<string, unknown> = {}) => ({
  bars: [0.2, 0.8, 0.1],
  durationSeconds: 65,
  isPaused: false,
  phase: 'recording',
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
  eventHandlers.clear();
});

describe('RecorderStrip', () => {
  it('renders nothing until a recorder state event arrives', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(false);
  });

  it('shows 3 bars and the timer while recording', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    sendState(recording());
    await flushPromises();
    expect(wrapper.findAll('.bar')).toHaveLength(3);
    expect(wrapper.find('.timer').text()).toBe('01:05');
  });

  it('pause button emits the tray pause event; resume when paused', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    sendState(recording());
    await flushPromises();
    await wrapper.find('.pause-btn').trigger('click');
    expect(emitEvent).toHaveBeenCalledWith('tray://pause-recording');

    sendState(recording({ isPaused: true }));
    await flushPromises();
    await wrapper.find('.pause-btn').trigger('click');
    expect(emitEvent).toHaveBeenCalledWith('tray://resume-recording');
  });

  it('stop button emits the tray stop event', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    sendState(recording());
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    expect(emitEvent).toHaveBeenCalledWith('tray://stop-recording');
  });

  it('shows the uploading spinner, then the success check', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    sendState(recording({ phase: 'uploading' }));
    await flushPromises();
    expect(wrapper.find('.spinner').exists()).toBe(true);

    sendState(recording({ phase: 'success' }));
    await flushPromises();
    expect(wrapper.find('.status-icon.ok').exists()).toBe(true);
  });

  it('hides after a closed phase', async () => {
    const wrapper = mount(RecorderStrip);
    await flushPromises();
    sendState(recording());
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(true);

    sendState(recording({ phase: 'closed' }));
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(false);
  });

  it('shows only when the displayed meeting is the one being recorded', async () => {
    const wrapper = mount(RecorderStrip, { props: { meetingId: '42' } });
    await flushPromises();
    sendState(recording({ meetingId: 42 }));
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(true);

    await wrapper.setProps({ meetingId: '7' });
    expect(wrapper.find('.strip').exists()).toBe(false);

    await wrapper.setProps({ meetingId: null });
    expect(wrapper.find('.strip').exists()).toBe(false);
  });

  it('shows regardless of selection when the recording has no meeting', async () => {
    const wrapper = mount(RecorderStrip, { props: { meetingId: '7' } });
    await flushPromises();
    sendState(recording({ meetingId: null }));
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(true);
  });
});
