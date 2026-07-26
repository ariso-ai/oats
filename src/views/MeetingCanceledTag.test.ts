// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import MeetingCanceledTag from './MeetingCanceledTag.vue';

describe('MeetingCanceledTag', () => {
  it('renders the "Canceled" label', () => {
    const wrapper = mount(MeetingCanceledTag);
    expect(wrapper.text()).toContain('Canceled');
  });
});
