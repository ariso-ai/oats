// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const list = vi.fn();
const checkSession = vi.fn();
const combineAndUpload = vi.fn();
const discardAll = vi.fn();

vi.mock('../tauri', () => ({
  auth: { checkSession: (...a: unknown[]) => checkSession(...a) },
  pending: { list: (...a: unknown[]) => list(...a) },
}));
vi.mock('../composables/usePendingUploads', () => ({
  combineAndUpload: (...a: unknown[]) => combineAndUpload(...a),
  discardAll: (...a: unknown[]) => discardAll(...a),
}));

import PendingUploads from './PendingUploads.vue';

const items = [
  { createdAt: '2026-06-12T09:00:00Z', startAt: '2026-06-12T09:00:00Z', endAt: '2026-06-12T09:05:00Z', durationSeconds: 300 },
  { createdAt: '2026-06-12T11:00:00Z', startAt: '2026-06-12T11:00:00Z', endAt: '2026-06-12T11:02:00Z', durationSeconds: 120 },
];

beforeEach(() => {
  vi.clearAllMocks();
  checkSession.mockResolvedValue({ user: { email: 'tester@example.com' } });
});

describe('PendingUploads', () => {
  it('renders nothing when there are no pending uploads', async () => {
    list.mockResolvedValue([]);
    const wrapper = mount(PendingUploads);
    await flushPromises();
    expect(wrapper.find('.pending').exists()).toBe(false);
  });

  it('lists items and shows the count on the Upload button', async () => {
    list.mockResolvedValue(items);
    const wrapper = mount(PendingUploads);
    await flushPromises();
    expect(wrapper.find('.pending-card').exists()).toBe(true);
    expect(wrapper.findAll('.pending-item')).toHaveLength(2);
    expect(wrapper.findAll('.pi-wave')).toHaveLength(2);
    expect(wrapper.find('.upload').text()).toContain('Upload (2)');
  });

  it('Upload combines+uploads, refreshes, and emits uploaded on success', async () => {
    list.mockResolvedValueOnce(items).mockResolvedValueOnce([]);
    combineAndUpload.mockResolvedValue(undefined);
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.upload').trigger('click');
    await flushPromises();

    expect(combineAndUpload).toHaveBeenCalledWith(items);
    expect(wrapper.emitted('uploaded')).toHaveLength(1);
    expect(wrapper.find('.pending').exists()).toBe(false);
  });

  it('shows an error and keeps items when upload fails', async () => {
    list.mockResolvedValue(items);
    combineAndUpload.mockRejectedValue(new Error('offline'));
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.upload').trigger('click');
    await flushPromises();

    expect(wrapper.find('.pending-error').exists()).toBe(true);
    expect(wrapper.find('.pending-error').text()).toBe('Upload failed — try again.');
    expect(wrapper.findAll('.pending-item')).toHaveLength(2);
  });

  it('explains missing session before retrying a pending upload', async () => {
    list.mockResolvedValue(items);
    checkSession.mockResolvedValue(null);
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.upload').trigger('click');
    await flushPromises();

    expect(combineAndUpload).not.toHaveBeenCalled();
    expect(wrapper.find('.pending-error').text()).toBe('Upload failed — sign in to Ari again, then retry.');
    expect(wrapper.findAll('.pending-item')).toHaveLength(2);
  });

  it('explains auth failures when retrying a pending upload', async () => {
    list.mockResolvedValue(items);
    combineAndUpload.mockRejectedValue(new Error('Failed to get presigned upload URL (401)'));
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.upload').trigger('click');
    await flushPromises();

    expect(wrapper.find('.pending-error').text()).toBe('Upload failed — sign in to Ari again, then retry.');
    expect(wrapper.findAll('.pending-item')).toHaveLength(2);
  });

  it('Discard all confirms then clears', async () => {
    list.mockResolvedValueOnce(items).mockResolvedValueOnce([]);
    discardAll.mockResolvedValue(undefined);
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.discard').trigger('click'); // first click → confirm
    expect(wrapper.find('.discard').text()).toContain('Confirm');
    await wrapper.find('.discard').trigger('click'); // second click → run
    await flushPromises();

    expect(discardAll).toHaveBeenCalledWith(items);
    expect(wrapper.find('.pending').exists()).toBe(false);
  });

  it('cancels a pending discard confirmation when the pointer leaves', async () => {
    list.mockResolvedValue(items);
    discardAll.mockResolvedValue(undefined);
    const wrapper = mount(PendingUploads);
    await flushPromises();

    await wrapper.find('.discard').trigger('click'); // arm confirm
    expect(wrapper.find('.discard').text()).toContain('Confirm');
    await wrapper.find('.pending-actions').trigger('mouseleave'); // move away
    expect(wrapper.find('.discard').text()).toContain('Discard all');

    // A subsequent click only re-arms — it must not discard.
    await wrapper.find('.discard').trigger('click');
    expect(discardAll).not.toHaveBeenCalled();
  });

  it('exposes refresh() to reload the list', async () => {
    list.mockResolvedValueOnce([]).mockResolvedValueOnce(items);
    const wrapper = mount(PendingUploads);
    await flushPromises();
    expect(wrapper.find('.pending').exists()).toBe(false);

    await (wrapper.vm as unknown as { refresh: () => Promise<void> }).refresh();
    await flushPromises();
    expect(wrapper.findAll('.pending-item')).toHaveLength(2);
  });
});
