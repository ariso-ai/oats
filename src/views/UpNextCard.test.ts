// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const getMeetingDetail = vi.fn();

// The card fetches the featured meeting's detail to surface attendees; mock the
// backend so the test controls the participant list it gets back.
vi.mock('../composables/useBackend', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../composables/useBackend')>();
  return {
    ...actual,
    getActiveBackend: () =>
      Promise.resolve({
        id: 'ariso',
        getMeetingDetail: (meeting: unknown) => getMeetingDetail(meeting),
      }),
  };
});

import UpNextCard from './UpNextCard.vue';

// Fixed "now"; meetings below are all in the future relative to it so they land
// in the UPCOMING bucket.
const NOW = new Date('2026-06-16T12:00:00Z');

function meeting(over: Record<string, unknown>) {
  return {
    id: 'm1',
    title: 'Meeting',
    timestamp: '2026-06-16T12:30:00Z',
    endTimestamp: '2026-06-16T13:15:00Z',
    ...over,
  };
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  getMeetingDetail.mockReset();
  getMeetingDetail.mockResolvedValue({ participants: [] });
});

function mountCard(meetings: Record<string, unknown>[]) {
  return mount(UpNextCard, { props: { meetings, now: NOW } });
}

describe('UpNextCard', () => {
  it('renders the serif date/time greeting with no space before AM/PM', async () => {
    const wrapper = mountCard([meeting({})]);
    await flushPromises();
    expect(wrapper.find('.greeting').text()).toMatch(/\d{1,2}:\d{2}(AM|PM)/i);
  });

  it('features the soonest upcoming meeting and lists the rest', async () => {
    const wrapper = mountCard([
      meeting({ id: 'a', title: 'Discovery call', timestamp: '2026-06-16T12:30:00Z' }),
      meeting({
        id: 'b',
        title: 'Market Trends Review',
        timestamp: '2026-06-16T16:00:00Z',
        endTimestamp: '2026-06-16T16:45:00Z',
      }),
      meeting({
        id: 'c',
        title: 'Product Update',
        timestamp: '2026-06-16T16:45:00Z',
        endTimestamp: '2026-06-16T17:15:00Z',
      }),
    ]);
    await flushPromises();
    expect(wrapper.find('.head-title').text()).toBe('Discovery call');
    const rows = wrapper.findAll('.meeting-row:not(.meeting-row--more)');
    expect(rows.map((r) => r.find('.row-title').text())).toEqual([
      'Market Trends Review',
      'Product Update',
    ]);
    // 45-minute span derived from start/end.
    expect(rows[0].find('.row-sub').text()).toMatch(/45min/);
  });

  it('collapses extra meetings into a "N more…" tail', async () => {
    const items = Array.from({ length: 8 }, (_, i) =>
      meeting({ id: `m${i}`, title: `Meeting ${i}`, timestamp: `2026-06-16T1${i}:00:00Z` })
    );
    const wrapper = mountCard(items);
    await flushPromises();
    // featured + 4 visible rows + 3 collapsed.
    expect(wrapper.findAll('.meeting-row:not(.meeting-row--more)')).toHaveLength(4);
    expect(wrapper.find('.meeting-row--more').text()).toContain('3 more');
  });

  it('renders attendee initials with an overflow pill', async () => {
    getMeetingDetail.mockResolvedValue({
      participants: [
        { name: 'Ada Lovelace' },
        { name: 'Grace Hopper' },
        { name: 'Alan Turing' },
        { name: 'Edsger Dijkstra' },
      ],
    });
    const wrapper = mountCard([meeting({ id: 'a', title: 'Discovery call' })]);
    await flushPromises();
    const avatars = wrapper.findAll('.avatars .avatar:not(.avatar--more)');
    expect(avatars).toHaveLength(3);
    expect(avatars[0].text()).toBe('AL');
    expect(wrapper.find('.avatar--more').text()).toBe('+1');
  });

  it('uses the avatar image when a participant has an avatar url', async () => {
    getMeetingDetail.mockResolvedValue({
      participants: [{ name: 'Ada Lovelace', avatarUrl: 'https://example.com/ada.png' }],
    });
    const wrapper = mountCard([meeting({ id: 'a' })]);
    await flushPromises();
    const img = wrapper.find('img.avatar');
    expect(img.exists()).toBe(true);
    expect(img.attributes('src')).toBe('https://example.com/ada.png');
  });

  it('emits start and select for the featured meeting', async () => {
    const wrapper = mountCard([
      meeting({ id: 'a', title: 'Discovery call' }),
      meeting({ id: 'b', title: 'Later', timestamp: '2026-06-16T16:00:00Z' }),
    ]);
    await flushPromises();
    await wrapper.find('.action-btn').trigger('click');
    await wrapper.find('.head-title').trigger('click');
    expect(wrapper.emitted('start')?.[0]?.[0]).toMatchObject({ id: 'a' });
    expect(wrapper.emitted('select')?.[0]?.[0]).toMatchObject({ id: 'a' });
  });

  it('emits record when the Impromptu Meeting button is clicked', async () => {
    const wrapper = mountCard([meeting({ id: 'a' })]);
    await flushPromises();
    await wrapper.find('.impromptu-btn').trigger('click');
    expect(wrapper.emitted('record')).toHaveLength(1);
  });

  it('shows the Impromptu Meeting button even with no upcoming meetings', async () => {
    const wrapper = mountCard([meeting({ id: 'past', timestamp: '2020-01-01T00:00:00Z', endTimestamp: '2020-01-01T01:00:00Z' })]);
    await flushPromises();
    expect(wrapper.find('.impromptu-btn').exists()).toBe(true);
  });

  it('pages between upcoming meetings with the chevrons', async () => {
    const wrapper = mountCard([
      meeting({ id: 'a', title: 'First', timestamp: '2026-06-16T12:30:00Z' }),
      meeting({ id: 'b', title: 'Second', timestamp: '2026-06-16T16:00:00Z' }),
    ]);
    await flushPromises();
    expect(wrapper.find('.head-title').text()).toBe('First');
    await wrapper.find('[aria-label="Next meeting"]').trigger('click');
    await flushPromises();
    expect(wrapper.find('.head-title').text()).toBe('Second');
  });

  it('shows the empty state when nothing is upcoming', async () => {
    const wrapper = mountCard([meeting({ id: 'past', timestamp: '2020-01-01T00:00:00Z', endTimestamp: '2020-01-01T01:00:00Z' })]);
    await flushPromises();
    expect(wrapper.find('.up-next-empty').exists()).toBe(true);
    expect(wrapper.find('.card').exists()).toBe(false);
  });
});
