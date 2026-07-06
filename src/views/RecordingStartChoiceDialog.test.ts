// @vitest-environment jsdom
import { describe, it, expect, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';
import RecordingStartChoiceDialog from './RecordingStartChoiceDialog.vue';

enableAutoUnmount(afterEach);

describe('RecordingStartChoiceDialog', () => {
  it('renders the meeting title and emits continue / new / cancel', async () => {
    const wrapper = mount(RecordingStartChoiceDialog, {
      props: { open: true, meetingTitle: 'Weekly Sync' },
    });
    expect(wrapper.text()).toContain('Weekly Sync');

    await wrapper.find('.rec-choice__continue').trigger('click');
    await wrapper.find('.rec-choice__new').trigger('click');
    await wrapper.find('.rec-choice__cancel').trigger('click');

    expect(wrapper.emitted('continue')).toHaveLength(1);
    expect(wrapper.emitted('new')).toHaveLength(1);
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });

  it('renders nothing when closed', () => {
    const wrapper = mount(RecordingStartChoiceDialog, {
      props: { open: false, meetingTitle: 'X' },
    });
    expect(wrapper.find('.rec-choice').exists()).toBe(false);
  });
});
