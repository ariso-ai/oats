// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import RecordingDeleteConfirmDialog from './RecordingDeleteConfirmDialog.vue';

describe('RecordingDeleteConfirmDialog', () => {
  it('renders nothing when closed', () => {
    const w = mount(RecordingDeleteConfirmDialog, { props: { open: false } });
    expect(w.find('[role="dialog"]').exists()).toBe(false);
  });

  it('emits confirm and cancel from its buttons', async () => {
    const w = mount(RecordingDeleteConfirmDialog, { props: { open: true } });
    await w.find('.danger-btn').trigger('click');
    await w.find('.secondary-btn').trigger('click');
    expect(w.emitted('confirm')).toHaveLength(1);
    expect(w.emitted('cancel')).toHaveLength(1);
  });
});
