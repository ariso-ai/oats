// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { MeetingDetail, MeetingListItem } from '../composables/useBackend';

const getMeetingDetail = vi.fn();
const getMeetingTranscript = vi.fn();
const updateMeetingNotesTitle = vi.fn();
const notesCanEdit = vi.fn(() => false);
const loadNote = vi.fn();
const saveNote = vi.fn();

vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () =>
    Promise.resolve({
      getMeetingDetail: (i: MeetingListItem) => getMeetingDetail(i),
      getMeetingTranscript: (i: MeetingListItem) => getMeetingTranscript(i),
    }),
}));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({ updateMeetingNotesTitle: (...a: unknown[]) => updateMeetingNotesTitle(...a) }),
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
  updateMeetingNotesTitle.mockResolvedValue(undefined);
  getMeetingTranscript.mockResolvedValue(null);
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

    expect(updateMeetingNotesTitle).toHaveBeenCalledWith('7', 'New title');
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

    expect(updateMeetingNotesTitle).not.toHaveBeenCalled();
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('cancels editing on Escape without calling the API', async () => {
    const wrapper = await mountWith(detail());
    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');

    await input.setValue('Discarded');
    await input.trigger('keydown', { key: 'Escape' });
    await flushPromises();

    expect(updateMeetingNotesTitle).not.toHaveBeenCalled();
    expect(wrapper.find('input.head-title--input').exists()).toBe(false);
    expect(wrapper.find('.head-title').text()).toBe('Old title');
  });

  it('keeps the title plain (not editable) for a local recording', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi' }));
    expect(wrapper.find('.head-title--editable').exists()).toBe(false);
    await wrapper.find('.head-title').trigger('click');
    expect(wrapper.find('input.head-title--input').exists()).toBe(false);
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
    updateMeetingNotesTitle.mockRejectedValue(new Error('boom'));
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const wrapper = await mountWith(detail());

    await wrapper.find('.head-title').trigger('click');
    const input = wrapper.find('input.head-title--input');
    await input.setValue('New title');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(updateMeetingNotesTitle).toHaveBeenCalledOnce();
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
