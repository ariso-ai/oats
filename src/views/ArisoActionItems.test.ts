// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import type { MeetingActionItem, MeetingParticipantInfo } from '../composables/useBackend';

const apiRequest = vi.fn();
const todosChanged = vi.fn();

vi.mock('../tauri', () => ({
  api: { request: (...a: unknown[]) => apiRequest(...a) },
}));

import ArisoActionItems from './ArisoActionItems.vue';

const me: MeetingParticipantInfo = { name: 'Shawn Zhu', self: true, meetingParticipantId: 1 };
const dana: MeetingParticipantInfo = { name: 'Dana', meetingParticipantId: 2 };

type Reply = { status: number; data: unknown };
type Route = (method: string, url: string, body?: unknown) => Reply | Promise<Reply> | undefined;

/** Answer API calls from `route`; anything it doesn't claim is an empty
 *  follow-up lookup. */
function serve(route: Route = () => undefined) {
  apiRequest.mockImplementation(async (method: string, url: string, body?: unknown) => {
    const reply = await route(method, url, body);
    if (reply) return reply;
    if (method === 'GET' && url.startsWith('/follow-ups/by-source')) {
      return { status: 200, data: { followUps: [] } };
    }
    throw new Error(`unexpected ${method} ${url}`);
  });
}

function followUpsReply(rows: Array<{ id: number; description: string; completed?: boolean }>): Reply {
  return {
    status: 200,
    data: {
      followUps: rows.map((r) => ({
        id: r.id,
        raw: { description: r.description, completed: !!r.completed },
      })),
    },
  };
}

async function mountWith(
  items: MeetingActionItem[],
  participants: MeetingParticipantInfo[] = [me, dana]
): Promise<VueWrapper> {
  const wrapper = mount(ArisoActionItems, {
    props: { meetingId: '9', meetingTitle: 'Platform Sync', items, participants, todosChanged },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

/** The list row for an item, found by its text. */
function row(wrapper: VueWrapper, text: string) {
  const li = wrapper.findAll('li').find((l) => l.find('.ai-text').text() === text);
  if (!li) throw new Error(`no row for "${text}"`);
  return li;
}

function calls(method: string, prefix: string) {
  return apiRequest.mock.calls.filter((c) => c[0] === method && String(c[1]).startsWith(prefix));
}

beforeEach(() => {
  apiRequest.mockReset();
  todosChanged.mockReset();
  document.body.innerHTML = '';
});

describe('ArisoActionItems', () => {
  it('groups items under their owners', async () => {
    serve();
    const wrapper = await mountWith([
      { name: 'Dana', item: 'Send pricing deck' },
      { name: 'Shawn Zhu', item: 'Draft the RFC' },
      { item: 'Loose item' },
    ]);

    expect(wrapper.findAll('.ai-name').map((n) => n.text())).toEqual(['Dana', 'Shawn Zhu']);
    expect(wrapper.findAll('.ai-text').map((n) => n.text())).toEqual([
      'Send pricing deck',
      'Draft the RFC',
      'Loose item',
    ]);
    expect(apiRequest).toHaveBeenCalledWith(
      'GET',
      '/follow-ups/by-source?source_id=9&source_type=action_item'
    );
  });

  it('shows an item checked off when its follow-up is complete', async () => {
    serve((m, u) => (u.startsWith('/follow-ups/by-source') ? followUpsReply([
      { id: 5, description: 'Send pricing deck', completed: true },
    ]) : undefined));
    const wrapper = await mountWith([{ name: 'Dana', item: 'Send pricing deck' }]);

    const li = row(wrapper, 'Send pricing deck');
    expect((li.find('input[type="checkbox"]').element as HTMLInputElement).checked).toBe(true);
    expect(li.classes()).toContain('ai-done');
  });

  it('checks off my own item in one click: creates its follow-up, completes it, and says so', async () => {
    serve((m, u) => {
      if (m === 'POST' && u === '/follow-ups') {
        return { status: 201, data: { followUp: { id: 11, raw: { description: 'Draft the RFC' } } } };
      }
      if (m === 'PATCH' && u === '/follow-ups/11/complete') return { status: 200, data: {} };
    });
    const wrapper = await mountWith([
      { id: 'a1', name: 'Shawn Zhu', item: 'Draft the RFC', meetingParticipantId: 1 },
    ]);

    const box = row(wrapper, 'Draft the RFC').find('input[type="checkbox"]');
    await box.setValue(true);
    await flushPromises();

    expect(calls('POST', '/follow-ups')[0][2]).toMatchObject({
      description: 'Draft the RFC',
      sourceId: '9',
      sourceType: 'action_item',
      reasoning: 'Action item from meeting: Platform Sync',
    });
    expect(calls('PATCH', '/follow-ups/11/complete')[0][2]).toEqual({ completed: true });
    expect(row(wrapper, 'Draft the RFC').classes()).toContain('ai-done');
    expect(todosChanged).toHaveBeenCalledWith('9');
  });

  it('still reports the change when the write lands after the list was unmounted', async () => {
    // Switching meetings unmounts this list (the detail panel clears while it
    // loads), and Vue drops emits from an unmounted component — but the write
    // succeeded, so the Todo list must still hear about it.
    let finishWrite!: (r: Reply) => void;
    serve((m, u) => {
      if (u.startsWith('/follow-ups/by-source')) {
        return followUpsReply([{ id: 5, description: 'Draft the RFC' }]);
      }
      if (m === 'PATCH') return new Promise<Reply>((resolve) => (finishWrite = resolve));
    });
    const wrapper = await mountWith([{ name: 'Shawn Zhu', item: 'Draft the RFC' }]);

    await row(wrapper, 'Draft the RFC').find('input[type="checkbox"]').setValue(true);
    await flushPromises();
    wrapper.unmount();
    finishWrite({ status: 200, data: {} });
    await flushPromises();

    expect(todosChanged).toHaveBeenCalledWith('9');
  });

  it('unticking reopens the item', async () => {
    serve((m, u) => {
      if (u.startsWith('/follow-ups/by-source')) {
        return followUpsReply([{ id: 5, description: 'Draft the RFC', completed: true }]);
      }
      if (m === 'PATCH' && u === '/follow-ups/5/complete') return { status: 200, data: {} };
    });
    const wrapper = await mountWith([{ name: 'Shawn Zhu', item: 'Draft the RFC' }]);

    await row(wrapper, 'Draft the RFC').find('input[type="checkbox"]').setValue(false);
    await flushPromises();

    expect(calls('PATCH', '/follow-ups/5/complete')[0][2]).toEqual({ completed: false });
    expect(calls('POST', '/follow-ups')).toHaveLength(0);
    expect(row(wrapper, 'Draft the RFC').classes()).not.toContain('ai-done');
    expect(todosChanged).toHaveBeenCalledWith('9');
  });

  it('offers a Follow-up, not a checkbox, on someone else’s item without one', async () => {
    serve((m, u) => {
      if (m === 'POST' && u === '/follow-ups') {
        return { status: 201, data: { followUp: { id: 12, raw: { description: 'Send pricing deck' } } } };
      }
    });
    const wrapper = await mountWith([
      { id: 'a2', name: 'Dana', item: 'Send pricing deck', meetingParticipantId: 2 },
    ]);

    const li = row(wrapper, 'Send pricing deck');
    expect(li.find('input[type="checkbox"]').exists()).toBe(false);
    const followUp = li.findAll('button').find((b) => b.text() === 'Follow-up');
    expect(followUp).toBeTruthy();

    await followUp!.trigger('click');
    await flushPromises();

    expect(calls('POST', '/follow-ups')).toHaveLength(1);
    expect(calls('PATCH', '/follow-ups/')).toHaveLength(0);
    const after = row(wrapper, 'Send pricing deck');
    expect((after.find('input[type="checkbox"]').element as HTMLInputElement).checked).toBe(false);
    expect(after.findAll('button').some((b) => b.text() === 'Follow-up')).toBe(false);
    // An open follow-up changes nobody's Todo list.
    expect(todosChanged).not.toHaveBeenCalled();
  });

  it('reverts the box and explains when the completion write fails', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    serve((m, u) => {
      if (u.startsWith('/follow-ups/by-source')) {
        return followUpsReply([{ id: 5, description: 'Draft the RFC' }]);
      }
      if (m === 'PATCH') return { status: 500, data: { error: 'boom' } };
    });
    const wrapper = await mountWith([{ name: 'Shawn Zhu', item: 'Draft the RFC' }]);

    await row(wrapper, 'Draft the RFC').find('input[type="checkbox"]').setValue(true);
    await flushPromises();

    const box = row(wrapper, 'Draft the RFC').find('input[type="checkbox"]');
    expect((box.element as HTMLInputElement).checked).toBe(false);
    expect(row(wrapper, 'Draft the RFC').classes()).not.toContain('ai-done');
    expect(wrapper.find('[role="alert"]').text()).toContain('Could not update');
    expect(todosChanged).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it('hides follow-up controls when the follow-up lookup fails, rather than risk duplicates', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    serve((m, u) => (u.startsWith('/follow-ups/by-source') ? { status: 500, data: {} } : undefined));
    const wrapper = await mountWith([
      { id: 'a1', name: 'Shawn Zhu', item: 'Draft the RFC', meetingParticipantId: 1 },
    ]);

    const li = row(wrapper, 'Draft the RFC');
    expect(li.find('input[type="checkbox"]').exists()).toBe(false);
    expect(li.findAll('button').some((b) => b.text() === 'Follow-up')).toBe(false);
    // Reassigning doesn't depend on follow-ups, so it stays.
    expect(li.findAll('button').some((b) => b.text() === 'Reassign')).toBe(true);
    errorSpy.mockRestore();
  });

  it('reassigns an item from the menu and moves it under its new owner', async () => {
    serve((m, u) => {
      if (m === 'PATCH' && u === '/meeting-notes/9/action-items/a2/assignee') {
        return {
          status: 200,
          data: { actionItem: { id: 'a2', name: 'Shawn Zhu', meetingParticipantId: 1 } },
        };
      }
    });
    const wrapper = await mountWith([
      { id: 'a2', name: 'Dana', item: 'Send pricing deck', meetingParticipantId: 2 },
    ]);

    const reassign = row(wrapper, 'Send pricing deck')
      .findAll('button')
      .find((b) => b.text() === 'Reassign')!;
    await reassign.trigger('click');
    const options = wrapper.findAll('[role="menuitem"]');
    expect(options.map((o) => o.find('.ai-menu-name').text())).toEqual([
      'Shawn Zhu',
      'Dana',
      'Unassigned',
    ]);

    await options[0].trigger('click');
    await flushPromises();

    expect(calls('PATCH', '/meeting-notes/9/action-items/a2/assignee')[0][2]).toEqual({
      meetingParticipantId: 1,
    });
    expect(wrapper.findAll('.ai-name').map((n) => n.text())).toEqual(['Shawn Zhu']);
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
    // Now mine, so it can be checked off directly.
    expect(row(wrapper, 'Send pricing deck').find('input[type="checkbox"]').exists()).toBe(true);
    expect(todosChanged).toHaveBeenCalledWith('9');
  });

  it('puts a reassigned item back and explains when the server refuses', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    serve((m) => (m === 'PATCH' ? { status: 403, data: { error: 'Not a meeting attendee' } } : undefined));
    const wrapper = await mountWith([
      { id: 'a2', name: 'Dana', item: 'Send pricing deck', meetingParticipantId: 2 },
    ]);

    await row(wrapper, 'Send pricing deck')
      .findAll('button')
      .find((b) => b.text() === 'Reassign')!
      .trigger('click');
    await wrapper.findAll('[role="menuitem"]')[0].trigger('click');
    await flushPromises();

    expect(wrapper.findAll('.ai-name').map((n) => n.text())).toEqual(['Dana']);
    expect(wrapper.find('[role="alert"]').text()).toContain('Could not reassign to Shawn Zhu');
    expect(todosChanged).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it('offers no Reassign for items the server cannot move, or with nobody to move them to', async () => {
    serve();
    const legacy = await mountWith([{ name: 'Dana', item: 'Legacy blob item' }]);
    expect(legacy.findAll('button').some((b) => b.text() === 'Reassign')).toBe(false);

    const alone = await mountWith(
      [{ id: 'a1', name: 'Dana', item: 'Ship it', meetingParticipantId: null }],
      [{ name: 'Guest', email: 'g@x.com' }]
    );
    expect(alone.findAll('button').some((b) => b.text() === 'Reassign')).toBe(false);
  });

  it('reloads follow-ups when switching meetings', async () => {
    serve((m, u) => (u.includes('source_id=9') ? followUpsReply([
      { id: 5, description: 'Ship it', completed: true },
    ]) : undefined));
    const wrapper = await mountWith([{ name: 'Shawn Zhu', item: 'Ship it' }]);
    expect(row(wrapper, 'Ship it').classes()).toContain('ai-done');

    await wrapper.setProps({ meetingId: '10' });
    await flushPromises();

    expect(apiRequest).toHaveBeenCalledWith(
      'GET',
      '/follow-ups/by-source?source_id=10&source_type=action_item'
    );
    expect(row(wrapper, 'Ship it').classes()).not.toContain('ai-done');
  });
});
