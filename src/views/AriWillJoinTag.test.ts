// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import AriWillJoinTag from './AriWillJoinTag.vue';

describe('AriWillJoinTag', () => {
  it('renders the "Ari will join" label', () => {
    const wrapper = mount(AriWillJoinTag);
    expect(wrapper.text()).toContain('Ari will join');
    expect(wrapper.find('.ari-tag--joined').exists()).toBe(false);
  });

  it('renders the present tense once Ari has joined', () => {
    const wrapper = mount(AriWillJoinTag, { props: { joined: true } });
    expect(wrapper.text()).toContain('Ari has joined');
    expect(wrapper.text()).not.toContain('will join');
    expect(wrapper.find('.ari-tag--joined').exists()).toBe(true);
    expect(wrapper.find('.ari-tag').attributes('title')).toContain('has joined');
  });
});
