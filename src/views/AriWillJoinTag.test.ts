// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import AriWillJoinTag from './AriWillJoinTag.vue';

describe('AriWillJoinTag', () => {
  it('renders the "Ari will join" label', () => {
    const wrapper = mount(AriWillJoinTag);
    expect(wrapper.text()).toContain('Ari will join');
  });
});
