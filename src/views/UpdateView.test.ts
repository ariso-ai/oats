// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const mocks = vi.hoisted(() => {
  const defaultState = {
    auto_check_enabled: true,
    last_check_unix: null,
    skipped_version: null,
    snoozed_until_unix: null,
    latest_known: null,
  };

  return {
    state: { ...defaultState },
    defaultState,
    closeWindow: vi.fn(),
    listeners: new Map<string, (e: { payload: unknown }) => void>(),
    installAndRelaunch: vi.fn(),
    skipVersion: vi.fn(),
    snooze: vi.fn(),
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    mocks.listeners.set(name, cb);
    return Promise.resolve(() => mocks.listeners.delete(name));
  },
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close: mocks.closeWindow }),
}));

vi.mock('../tauri', () => ({
  updater: {
    getState: () => Promise.resolve(mocks.state),
    installAndRelaunch: mocks.installAndRelaunch,
    skipVersion: mocks.skipVersion,
    snooze: mocks.snooze,
  },
}));

import UpdateView from './UpdateView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.clear();
  mocks.state = { ...mocks.defaultState };
});

describe('UpdateView', () => {
  it('shows an up-to-date state without an install button when no update exists', async () => {
    const wrapper = mount(UpdateView);
    await flushPromises();

    expect(wrapper.text()).toContain('Oats is up to date');
    expect(wrapper.text()).toContain('Version 0.4.0 is installed.');
    expect(wrapper.text()).not.toContain('Install Update');
    expect(wrapper.text()).toContain('Done');
  });

  it('renders updater markdown directly for a newer version', async () => {
    mocks.state = {
      ...mocks.defaultState,
      latest_known: {
        version: '0.4.1',
        mandatory: false,
        notes: [
          '## [0.4.1](https://github.com/ariso-ai/oats/compare/v0.4.0...v0.4.1)',
          '',
          '### Features',
          '- Release-note powered highlights',
          '- Smaller native update window',
          '- Up-to-date state with no install CTA',
        ].join('\n'),
      },
    };

    const wrapper = mount(UpdateView);
    await flushPromises();

    expect(wrapper.text()).toContain('What’s new for Oats?');
    expect(wrapper.text()).toContain('Version 0.4.1 is ready. You have 0.4.0.');
    expect(wrapper.text()).toContain('Features');
    expect(wrapper.text()).toContain('Release-note powered highlights');
    expect(wrapper.text()).toContain('Smaller native update window');
    expect(wrapper.text()).toContain('Up-to-date state with no install CTA');
    expect(wrapper.text()).not.toContain('Faster meeting notes');
    expect(wrapper.text()).toContain('Install Update');
  });

  it('treats a same-version update snapshot as current', async () => {
    mocks.state = {
      ...mocks.defaultState,
      latest_known: {
        version: '0.4.0',
        mandatory: false,
        notes: '- Should not show as pending',
      },
    };

    const wrapper = mount(UpdateView);
    await flushPromises();

    expect(wrapper.text()).toContain('Oats is up to date');
    expect(wrapper.text()).not.toContain('Install Update');
    expect(wrapper.text()).not.toContain('Should not show as pending');
  });

  it('can seed the dev download state for native visual review', async () => {
    mocks.state = {
      ...mocks.defaultState,
      latest_known: {
        version: '0.4.1',
        mandatory: false,
        notes: '### Features\n\n- Release-note powered highlights',
      },
    };

    const wrapper = mount(UpdateView);
    await flushPromises();

    mocks.listeners.get('update://debug-download-progress')?.({
      payload: { downloaded: 42, total: 100 },
    });
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain('Downloading update');
    expect(wrapper.text()).toContain('42%');
    expect(wrapper.text()).not.toContain('Install Update');
  });
});
