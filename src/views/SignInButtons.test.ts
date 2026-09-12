// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import SignInButtons from './SignInButtons.vue';

describe('SignInButtons', () => {
  it('emits the clicked provider', async () => {
    const wrapper = mount(SignInButtons, { props: { signingInWith: null } });

    await wrapper.get('.google-btn').trigger('click');
    await wrapper.get('.microsoft-btn').trigger('click');

    expect(wrapper.emitted('sign-in')).toEqual([['google'], ['microsoft']]);
    expect(wrapper.find('.sign-in-cancel').exists()).toBe(false);
    expect(wrapper.find('.sign-in-error').exists()).toBe(false);
  });

  it('disables both providers while one is pending and offers Cancel', async () => {
    const wrapper = mount(SignInButtons, { props: { signingInWith: 'microsoft' } });

    expect(wrapper.get('.google-btn').attributes('disabled')).toBeDefined();
    expect(wrapper.get('.microsoft-btn').attributes('disabled')).toBeDefined();
    expect(wrapper.get('.microsoft-btn').text()).toContain('Continue in your browser…');
    expect(wrapper.get('.google-btn').text()).toContain('Sign in with Google');

    await wrapper.get('.sign-in-cancel').trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });

  it('shows the error inline', () => {
    const wrapper = mount(SignInButtons, {
      props: { signingInWith: null, errorMessage: 'API returned 500' },
    });

    const error = wrapper.get('.sign-in-error');
    expect(error.text()).toBe('API returned 500');
    expect(error.attributes('role')).toBe('alert');
  });
});
