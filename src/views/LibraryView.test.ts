// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const listRecordings = vi.fn();
vi.mock('../tauri', () => ({
  local: { listRecordings: () => listRecordings() },
}));

import LibraryView from './LibraryView.vue';

beforeEach(() => vi.clearAllMocks());

describe('LibraryView', () => {
  it('shows an empty state when there are no recordings', async () => {
    listRecordings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).toContain('No recordings yet');
    expect(wrapper.findAll('.recording-row')).toHaveLength(0);
  });

  it('renders a row per recording in the order returned', async () => {
    listRecordings.mockResolvedValue([
      { id: 'b', title: 'Second', createdAt: '2026-06-02T10:00:00Z', durationSeconds: 75, status: 'done' },
      { id: 'a', title: 'First', createdAt: '2026-06-01T10:00:00Z', durationSeconds: 3661, status: 'failed' },
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const rows = wrapper.findAll('.recording-row');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('Second');
    expect(rows[0].text()).toContain('01:15'); // 75s
    expect(rows[1].text()).toContain('First');
    expect(rows[1].text()).toContain('61:01'); // 3661s
    expect(rows[1].text()).toContain('failed');
  });
});
