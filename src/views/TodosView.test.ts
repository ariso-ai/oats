// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import type { ActionItemEntry, ActionItemRef, MeetingListItem } from '../composables/useBackend';
import TodosView from './TodosView.vue';

const now = new Date(2026, 8, 18, 15, 0);

const standup: MeetingListItem = { id: 'm1', title: 'Daily Standup', timestamp: '2026-09-18T09:30:00' };
const sync: MeetingListItem = { id: 'm2', title: 'Product Sync', timestamp: '2026-09-17T11:00:00' };

const ref = (text: string, followUpId?: string): ActionItemRef => ({
  kind: 'ariso',
  meetingId: 'm1',
  meetingTitle: 'Daily Standup',
  text,
  ...(followUpId ? { followUpId } : {}),
});

function entries(): ActionItemEntry[] {
  return [
    {
      meeting: standup,
      items: [
        { item: 'Follow up with Sarah', ref: ref('Follow up with Sarah') },
        { item: 'Write the retro doc', ref: ref('Write the retro doc') },
        {
          item: 'Book the offsite venue',
          ref: ref('Book the offsite venue', '9'),
          completion: { by: 'user', at: '2026-09-18T10:00:00' },
        },
      ],
    },
    { meeting: sync, items: [{ item: 'Send the deck', ref: ref('Send the deck') }, { item: 'Legacy bullet' }] },
  ];
}

const setDone = vi.fn();
const listChanged = vi.fn();

function mountView(props: Partial<InstanceType<typeof TodosView>['$props']> = {}): VueWrapper {
  return mount(TodosView, {
    props: {
      entries: entries(),
      loading: false,
      error: null,
      now,
      filterMeeting: null,
      completedWindowDays: 14,
      setDone,
      listChanged,
      ...props,
    },
    attachTo: document.body,
  });
}

function row(w: VueWrapper, text: string) {
  const li = w.findAll('li.todo-row').find((l) => l.find('.todo-text').text() === text);
  if (!li) throw new Error(`no row "${text}"`);
  return li;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  setDone.mockReset();
  listChanged.mockReset();
  setDone.mockImplementation(async (r: ActionItemRef) => ({ ...r, followUpId: 'new' }));
});

afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = '';
});

describe('TodosView', () => {
  it('lists open todos under day headers with their source meeting', () => {
    const w = mountView();
    const headers = w.findAll('.todos-sec-h').map((h) => h.text());
    expect(headers[0]).toContain('TODAY');
    expect(headers[1]).toContain('YESTERDAY');
    expect(row(w, 'Follow up with Sarah').find('.todo-sub').text()).toContain('Daily Standup');
    expect(w.find('[data-testid="todos-open-count"]').text()).toBe('4 open');
  });

  it('keeps Completed collapsed with a count until expanded, with a provenance line', async () => {
    const w = mountView();
    const disclosure = w.find('.todos-disclosure');
    expect(disclosure.text()).toContain('Completed · last 14 days');
    expect(disclosure.text()).toContain('1');
    expect(w.text()).not.toContain('Book the offsite venue');

    await disclosure.trigger('click');

    const done = row(w, 'Book the offsite venue');
    expect(done.classes()).toContain('todo-row--done');
    expect(done.find('.todo-prov').text()).toBe('Done by you · Fri Sep 18');
  });

  it('strikes a checked row in place, then hands the list over for a rebuild after the Undo window', async () => {
    const w = mountView();
    await row(w, 'Follow up with Sarah').find('input').setValue(true);
    await flushPromises();

    expect(setDone).toHaveBeenCalledWith(ref('Follow up with Sarah'), true);
    // Still in Open, struck through.
    const struck = row(w, 'Follow up with Sarah');
    expect(struck.classes()).toContain('todo-row--done');
    expect(w.findAll('.todos-sec')[0].text()).toContain('Follow up with Sarah');
    expect(w.find('.todos-toast').text()).toContain('Completed "Follow up with Sarah"');
    expect(w.find('[data-testid="todos-open-count"]').text()).toBe('3 open');
    expect(listChanged).not.toHaveBeenCalled();

    vi.advanceTimersByTime(5000);
    await flushPromises();

    expect(w.find('.todos-toast').exists()).toBe(false);
    expect(listChanged).toHaveBeenCalledTimes(1);
  });

  it('folds the row into Completed when the rebuilt list says it is done', async () => {
    const w = mountView();
    await row(w, 'Follow up with Sarah').find('input').setValue(true);
    await flushPromises();
    vi.advanceTimersByTime(5000);

    const rebuilt = entries();
    rebuilt[0].items[0] = { ...rebuilt[0].items[0], completion: { by: 'user' } };
    await w.setProps({ entries: rebuilt });
    await w.find('.todos-disclosure').trigger('click');

    const sections = w.findAll('.todos-sec');
    expect(sections[0].text()).not.toContain('Follow up with Sarah');
    expect(w.find('.todos-sec--completed').text()).toContain('Follow up with Sarah');
    expect(row(w, 'Follow up with Sarah').find('input').element.checked).toBe(true);
  });

  it('Undo reverses the write with the ref the first write returned', async () => {
    const w = mountView();
    await row(w, 'Follow up with Sarah').find('input').setValue(true);
    await flushPromises();

    await w.find('.todos-toast-undo').trigger('click');
    await flushPromises();

    expect(setDone).toHaveBeenLastCalledWith({ ...ref('Follow up with Sarah'), followUpId: 'new' }, false);
    expect(row(w, 'Follow up with Sarah').classes()).not.toContain('todo-row--done');
    expect(w.find('.todos-toast').exists()).toBe(false);
    expect(listChanged).toHaveBeenCalledTimes(1);
  });

  it('reverts the row and shows the inline error when the write fails', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    setDone.mockRejectedValue(new Error('500'));
    const w = mountView();
    const box = row(w, 'Follow up with Sarah').find('input');
    await box.setValue(true);
    await flushPromises();

    expect((box.element as HTMLInputElement).checked).toBe(false);
    expect(row(w, 'Follow up with Sarah').classes()).not.toContain('todo-row--done');
    expect(w.find('[role="alert"]').text()).toBe('Could not update "Follow up with Sarah". Please try again.');
    expect(w.find('.todos-toast').exists()).toBe(false);
    errorSpy.mockRestore();
  });

  it('reopens a completed row from its checkbox', async () => {
    const w = mountView();
    await w.find('.todos-disclosure').trigger('click');
    await row(w, 'Book the offsite venue').find('input').setValue(false);
    await flushPromises();

    expect(setDone).toHaveBeenCalledWith(ref('Book the offsite venue', '9'), false);
    expect(w.find('.todos-toast').text()).toContain('Reopened');
  });

  it('disables the checkbox of a todo it has no way to tick', () => {
    const w = mountView();
    expect((row(w, 'Legacy bullet').find('input').element as HTMLInputElement).disabled).toBe(true);
  });

  it('Space on a focused row toggles it; Enter opens its meeting', async () => {
    const w = mountView();
    const li = row(w, 'Send the deck');
    await li.trigger('keydown', { key: ' ' });
    await flushPromises();
    expect(setDone).toHaveBeenCalledWith(ref('Send the deck'), true);

    await li.trigger('keydown', { key: 'Enter' });
    expect(w.emitted('open-meeting')?.[0]).toEqual([sync]);
  });

  it('arrow keys move focus between rows', async () => {
    const w = mountView();
    const first = row(w, 'Follow up with Sarah');
    (first.element as HTMLElement).focus();
    await first.trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(row(w, 'Write the retro doc').element);
    await row(w, 'Write the retro doc').trigger('keydown', { key: 'ArrowUp' });
    expect(document.activeElement).toBe(first.element);
  });

  it('Open meeting emits the row’s source meeting', async () => {
    const w = mountView();
    await row(w, 'Send the deck').find('.todo-act').trigger('click');
    expect(w.emitted('open-meeting')?.[0]).toEqual([sync]);
  });

  it('filters to one meeting and names it in the header', async () => {
    const w = mountView({ filterMeeting: sync });
    expect(w.find('.head-title').text()).toBe('Todos · Product Sync');
    expect(w.findAll('li.todo-row').map((l) => l.find('.todo-text').text())).toEqual([
      'Send the deck',
      'Legacy bullet',
    ]);
    await w.find('.head-clear').trigger('click');
    expect(w.emitted('clear-filter')).toHaveLength(1);
  });

  it('rebuilds right away when the pane closes inside the Undo window', async () => {
    const w = mountView();
    await row(w, 'Follow up with Sarah').find('input').setValue(true);
    await flushPromises();
    w.unmount();
    expect(listChanged).toHaveBeenCalledTimes(1);
  });

  it('says where todos come from when there are none', () => {
    const w = mountView({ entries: [] });
    expect(w.text()).toContain('Action items from your meetings will show up here.');
    expect(w.find('[data-testid="todos-open-count"]').text()).toBe('0 open');
  });
});
