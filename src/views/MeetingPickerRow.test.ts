// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';

import MeetingPickerRow from './MeetingPickerRow.vue';

describe('MeetingPickerRow', () => {
  it('renders the title and the local start time', () => {
    const wrapper = mount(MeetingPickerRow, {
      props: { title: 'Standup', startAt: '2026-06-01T09:00:00Z' },
    });
    const expected = new Date('2026-06-01T09:00:00Z').toLocaleTimeString(undefined, {
      hour: 'numeric',
      minute: '2-digit',
    });
    expect(wrapper.text()).toContain('Standup');
    expect(wrapper.text()).toContain(expected);
  });

  it('falls back to "Untitled meeting" for a blank title', () => {
    const wrapper = mount(MeetingPickerRow, {
      props: { title: '', startAt: '2026-06-01T09:00:00Z' },
    });
    expect(wrapper.text()).toContain('Untitled meeting');
  });

  it('renders no time for an unparseable start', () => {
    const wrapper = mount(MeetingPickerRow, {
      props: { title: 'Standup', startAt: 'not-a-date' },
    });
    expect(wrapper.find('.meeting-time').text()).toBe('');
  });

  it('is a plain card by default and takes the featured treatment on request', () => {
    const plain = mount(MeetingPickerRow, {
      props: { title: 'Standup', startAt: '2026-06-01T09:00:00Z' },
    });
    expect(plain.classes()).not.toContain('meeting-row--featured');
    expect(plain.find('.meeting-badge').exists()).toBe(false);

    const featured = mount(MeetingPickerRow, {
      props: {
        title: 'Standup',
        startAt: '2026-06-01T09:00:00Z',
        featured: true,
        badge: 'Up next',
      },
    });
    expect(featured.classes()).toContain('meeting-row--featured');
    expect(featured.find('.meeting-badge').text()).toContain('Up next');
  });

  it('shows the live dot only while the meeting is happening now', () => {
    const upNext = mount(MeetingPickerRow, {
      props: { title: 'Standup', startAt: '2026-06-01T09:00:00Z', badge: 'Up next' },
    });
    expect(upNext.find('.live-dot').exists()).toBe(false);

    const live = mount(MeetingPickerRow, {
      props: {
        title: 'Standup',
        startAt: '2026-06-01T09:00:00Z',
        badge: 'Happening now',
        live: true,
      },
    });
    expect(live.find('.live-dot').exists()).toBe(true);
  });

  it('emits choose on click, and not when disabled', async () => {
    const wrapper = mount(MeetingPickerRow, {
      props: { title: 'Standup', startAt: '2026-06-01T09:00:00Z' },
    });
    await wrapper.trigger('click');
    expect(wrapper.emitted('choose')).toHaveLength(1);

    await wrapper.setProps({ disabled: true });
    expect(wrapper.attributes('disabled')).toBeDefined();
  });
});
