// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { MeetingDetail, MeetingListItem } from '../composables/useBackend';

const getMeetingDetail = vi.fn();
const getMeetingTranscript = vi.fn();
const renameMeeting = vi.fn();
const activeBackend = vi.fn();
const notesCanEdit = vi.fn(() => false);
const loadNote = vi.fn();
const saveNote = vi.fn();

vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () => activeBackend(),
}));
vi.mock('../composables/useMeetingNotesPersistence', () => ({
  useMeetingNotesPersistence: () => ({
    modeFor: () => 'local',
    canEdit: (meeting: MeetingListItem) => notesCanEdit(meeting),
    load: (meeting: MeetingListItem) => loadNote(meeting),
    save: (meeting: MeetingListItem, markdown: string) => saveNote(meeting, markdown),
  }),
}));

import MeetingDetailView from './MeetingDetailView.vue';

function detail(over: Partial<MeetingDetail> = {}): MeetingDetail {
  return {
    id: '7',
    title: 'Old title',
    startAt: '2026-06-02T10:00:00Z',
    participants: [],
    actionItems: [],
    isLocal: false,
    ...over,
  };
}

const item: MeetingListItem = { id: '7', title: 'Old title', timestamp: '2026-06-02T10:00:00Z' };

async function mountWith(d: MeetingDetail) {
  getMeetingDetail.mockResolvedValue(d);
  const wrapper = mount(MeetingDetailView, { props: { item } });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  vi.clearAllMocks();
  renameMeeting.mockResolvedValue(undefined);
  getMeetingTranscript.mockResolvedValue(null);
  activeBackend.mockResolvedValue({
    getMeetingDetail: (i: MeetingListItem) => getMeetingDetail(i),
    getMeetingTranscript: (i: MeetingListItem) => getMeetingTranscript(i),
    renameMeeting: (...a: unknown[]) => renameMeeting(...a),
  });
  notesCanEdit.mockReturnValue(false);
  loadNote.mockResolvedValue('');
  saveNote.mockResolvedValue(undefined);
});

describe('MeetingDetailView inline title editing', () => {
  it('renders an editable title for an Ariso meeting', async () => {
    const wrapper = await mountWith(detail());
    expect(wrapper.find('.head-title--editable').exists()).toBe(true);
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('commits a renamed title: calls the API, updates the heading, and emits titleUpdated', async () => {
    const wrapper = await mountWith(detail());

    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');
    expect(input.exists()).toBe(true);

    await input.setValue('New title');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).toHaveBeenCalledWith('7', 'New title');
    expect(wrapper.emitted('titleUpdated')?.[0]).toEqual([{ id: '7', title: 'New title' }]);
    expect(wrapper.find('.head-title').text()).toBe('New title');
    expect(wrapper.find('input.head-title--input').exists()).toBe(false);
  });

  it('does not call the API for an unchanged or whitespace-only title', async () => {
    const wrapper = await mountWith(detail());
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('   ');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).not.toHaveBeenCalled();
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('cancels editing on Escape without calling the API', async () => {
    const wrapper = await mountWith(detail());
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('Discarded');
    await input.trigger('keydown', { key: 'Escape' });
    await flushPromises();

    expect(renameMeeting).not.toHaveBeenCalled();
    expect(wrapper.find('input.head-title--input').exists()).toBe(false);
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('renames a local recording via the backend with the same inline UX', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi' }));
    expect(wrapper.find('.head-title--editable').exists()).toBe(true);

    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');
    expect(input.exists()).toBe(true);

    await input.setValue('Renamed local');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).toHaveBeenCalledWith('7', 'Renamed local');
    expect(wrapper.emitted('titleUpdated')?.[0]).toEqual([{ id: '7', title: 'Renamed local' }]);
    expect(wrapper.find('.head-title').text()).toBe('Renamed local');
  });

  it('shows a warning and blocks Enter when a local title exceeds 40 characters', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi' }));
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('a'.repeat(41));
    expect(wrapper.find('.head-title-error').exists()).toBe(true);
    expect(wrapper.find('.head-title-error').text()).toContain('40 characters or fewer');
    expect(wrapper.find('.head-title-error').text()).toContain('(41/40)');

    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).not.toHaveBeenCalled();
    // Editor stays open so the user can shorten the title.
    expect(wrapper.find('input.head-title--input').exists()).toBe(true);
  });

  it('reverts on blur while a local title is invalid', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi' }));
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('a'.repeat(41));
    await input.trigger('blur');
    await flushPromises();

    expect(renameMeeting).not.toHaveBeenCalled();
    expect(wrapper.find('input.head-title--input').exists()).toBe(false);
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('renames through the backend that loaded the detail, not the current setting', async () => {
    // Backend A serves the load; flipping Settings makes later resolutions
    // return backend B. The rename must stay on A.
    const renameA = vi.fn().mockResolvedValue(undefined);
    const renameB = vi.fn().mockResolvedValue(undefined);
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true, note: 'hi' }));
    const backendWith = (rename: typeof renameA) => ({
      getMeetingDetail: (i: MeetingListItem) => getMeetingDetail(i),
      getMeetingTranscript: (i: MeetingListItem) => getMeetingTranscript(i),
      renameMeeting: rename,
    });
    activeBackend.mockResolvedValueOnce(backendWith(renameA));
    activeBackend.mockResolvedValue(backendWith(renameB));

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');
    await input.setValue('Renamed after flip');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameA).toHaveBeenCalledWith('7', 'Renamed after flip');
    expect(renameB).not.toHaveBeenCalled();
  });

  it('commits a valid draft on blur', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi' }));
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('Blur saved');
    await input.trigger('blur');
    await flushPromises();

    expect(renameMeeting).toHaveBeenCalledWith('7', 'Blur saved');
    expect(wrapper.emitted('titleUpdated')?.[0]).toEqual([{ id: '7', title: 'Blur saved' }]);
    expect(wrapper.find('.head-title').text()).toBe('Blur saved');
  });

  it('does not length-limit ariso titles (server is the authority)', async () => {
    const wrapper = await mountWith(detail());
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    const long = 'a'.repeat(41);
    await input.setValue(long);
    expect(wrapper.find('.head-title-error').exists()).toBe(false);

    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).toHaveBeenCalledWith('7', long);
  });

  it('renders structured transcript chunks with timestamps for an Ariso meeting', async () => {
    getMeetingTranscript.mockResolvedValue([
      { chunk_index: 0, start_ms: 0, content: 'Speaker 1: Five is five bars. Should be' },
      { chunk_index: 1, start_ms: 3120, content: 'Speaker 1: should be three. Anyway' },
    ]);
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    // Transcript is the only available tab, so it loads on mount.
    await flushPromises();

    const lines = wrapper.findAll('.transcript-line');
    expect(lines).toHaveLength(2);
    expect(lines[0].find('.transcript-ts').text()).toBe('0:00');
    expect(lines[0].find('.transcript-content').text()).toBe('Speaker 1: Five is five bars. Should be');
    expect(lines[1].find('.transcript-ts').text()).toBe('0:03');
    expect(lines[1].find('.transcript-content').text()).toBe('Speaker 1: should be three. Anyway');
  });

  it('shows the empty state when an Ariso meeting has no transcript chunks', async () => {
    getMeetingTranscript.mockResolvedValue(null);
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    await flushPromises();
    expect(wrapper.find('.transcript-line').exists()).toBe(false);
    expect(wrapper.find('.content-empty').text()).toBe('No transcript available.');
  });

  it('keeps the editor open and does not emit when the API fails', async () => {
    renameMeeting.mockRejectedValue(new Error('boom'));
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const wrapper = await mountWith(detail());

    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');
    await input.setValue('New title');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(renameMeeting).toHaveBeenCalledOnce();
    expect(wrapper.emitted('titleUpdated')).toBeUndefined();
    expect(wrapper.find('input.head-title--input').exists()).toBe(true);
  });

  it('does not autosave an empty note before the selected note has loaded', async () => {
    vi.useFakeTimers();
    notesCanEdit.mockReturnValue(true);
    loadNote.mockResolvedValue('Already saved');
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true }));

    const localItem: MeetingListItem = {
      ...item,
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    mount(MeetingDetailView, { props: { item: localItem } });
    await flushPromises();

    await vi.advanceTimersByTimeAsync(800);
    expect(saveNote).not.toHaveBeenCalledWith(localItem, '');

    vi.useRealTimers();
  });

  it('loads My Notes when switching between meetings that keep My Notes selected', async () => {
    notesCanEdit.mockReturnValue(true);
    getMeetingDetail.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve(detail({ id: meeting.id, title: meeting.title, isLocal: true }))
    );
    loadNote.mockImplementation((meeting: MeetingListItem) => Promise.resolve(`note ${meeting.id}`));

    const first: MeetingListItem = {
      id: 'a',
      title: 'First',
      timestamp: '2026-06-02T10:00:00Z',
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    const second: MeetingListItem = {
      id: 'b',
      title: 'Second',
      timestamp: '2026-06-02T11:00:00Z',
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };

    const wrapper = mount(MeetingDetailView, { props: { item: first } });
    await flushPromises();
    expect(loadNote).toHaveBeenCalledWith(first);

    await wrapper.setProps({ item: second });
    await flushPromises();

    expect(loadNote).toHaveBeenCalledWith(second);
  });

  it('ignores stale note save completions after switching meetings', async () => {
    notesCanEdit.mockReturnValue(true);
    getMeetingDetail.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve(detail({ id: meeting.id, title: meeting.title, isLocal: true }))
    );
    loadNote.mockImplementation((meeting: MeetingListItem) => Promise.resolve(`note ${meeting.id}`));

    let resolveFirstSave: (() => void) | null = null;
    saveNote.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveFirstSave = resolve;
        })
    );

    const first: MeetingListItem = {
      id: 'a',
      title: 'First',
      timestamp: '2026-06-02T10:00:00Z',
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    const second: MeetingListItem = {
      id: 'b',
      title: 'Second',
      timestamp: '2026-06-02T11:00:00Z',
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };

    const wrapper = mount(MeetingDetailView, { props: { item: first } });
    await flushPromises();

    const save = (wrapper.vm as unknown as { saveNotesNow: () => Promise<void> }).saveNotesNow();
    await wrapper.setProps({ item: second });
    await flushPromises();

    resolveFirstSave?.();
    await save;
    await flushPromises();

    expect(saveNote).toHaveBeenCalledWith(first, 'note a');
    expect(wrapper.text()).toContain('Second');
    expect(wrapper.text()).not.toContain('First');
  });
});
