// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { MeetingDetail, MeetingListItem } from '../composables/useBackend';

const getMeetingDetail = vi.fn();
const getMeetingTranscript = vi.fn();
const renameMeeting = vi.fn();
const getMeetingAudio = vi.fn();
const activeBackend = vi.fn();
const notesCanEdit = vi.fn(() => false);
const loadNote = vi.fn();
const saveNote = vi.fn();
const shareTextNative = vi.fn();
const recordingStatus = vi.fn();
const readRecordingFile = vi.fn();
const retryTranscription = vi.fn();
const retryNotes = vi.fn();

vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () => activeBackend(),
}));
vi.mock('../composables/useMeetingNotesPersistence', () => ({
  useMeetingNotesPersistence: () => ({
    modeFor: () => 'local',
    canEdit: (meeting: MeetingListItem) => notesCanEdit(meeting),
    load: (meeting: MeetingListItem) => loadNote(meeting),
    save: (meeting: MeetingListItem, note: { content: string; title: string }) =>
      saveNote(meeting, note),
  }),
}));
vi.mock('../tauri', () => ({
  shareTextNative: (text: string, anchor: unknown) => shareTextNative(text, anchor),
  getDesktopConfig: () =>
    Promise.resolve({ webAppBaseUrl: 'https://app.test', pusherKey: '', pusherCluster: '' }),
  local: {
    recordingStatus: (id: string) => recordingStatus(id),
    readRecordingFile: (id: string, kind: string) => readRecordingFile(id, kind),
    retryTranscription: (id: string) => retryTranscription(id),
    retryNotes: (id: string) => retryNotes(id),
  },
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
  getMeetingAudio.mockResolvedValue(null);
  activeBackend.mockResolvedValue({
    getMeetingDetail: (i: MeetingListItem) => getMeetingDetail(i),
    getMeetingTranscript: (i: MeetingListItem) => getMeetingTranscript(i),
    renameMeeting: (...a: unknown[]) => renameMeeting(...a),
    getMeetingAudio: (i: MeetingListItem) => getMeetingAudio(i),
  });
  notesCanEdit.mockReturnValue(false);
  loadNote.mockResolvedValue({ content: '', title: '' });
  saveNote.mockResolvedValue(undefined);
  recordingStatus.mockResolvedValue({
    status: 'done',
    hasTranscript: false,
    hasNote: false,
    notesStatus: 'ready',
  });
  readRecordingFile.mockResolvedValue(null);
  retryTranscription.mockResolvedValue({ backend: 'local', id: '7', title: 'T', status: 'done' });
  retryNotes.mockResolvedValue(undefined);
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
    loadNote.mockResolvedValue({ content: 'Already saved', title: '' });
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true }));

    const localItem: MeetingListItem = {
      ...item,
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    mount(MeetingDetailView, { props: { item: localItem } });
    await flushPromises();

    await vi.advanceTimersByTimeAsync(800);
    expect(saveNote).not.toHaveBeenCalledWith(localItem, { content: '', title: '' });

    vi.useRealTimers();
  });

  it('edits the My-note title inline and autosaves it with the body', async () => {
    vi.useFakeTimers();
    notesCanEdit.mockReturnValue(true);
    loadNote.mockResolvedValue({ content: 'Body', title: 'Old note title' });
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true }));

    const localItem: MeetingListItem = {
      ...item,
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    const wrapper = mount(MeetingDetailView, { props: { item: localItem } });
    await flushPromises();

    expect(wrapper.find('.notes-title').text()).toBe('Old note title');
    await wrapper.find('.notes-title').trigger('click');
    const input = wrapper.find('input.notes-title--input');
    expect(input.exists()).toBe(true);
    await input.setValue('New note title');
    await vi.advanceTimersByTimeAsync(800);

    expect(saveNote).toHaveBeenCalledWith(localItem, { content: 'Body', title: 'New note title' });

    vi.useRealTimers();
  });

  it('shows the Untitled note placeholder when a loaded note has no title', async () => {
    notesCanEdit.mockReturnValue(true);
    loadNote.mockResolvedValue({ content: 'Body', title: '' });
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true }));

    const localItem: MeetingListItem = {
      ...item,
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    };
    const wrapper = mount(MeetingDetailView, { props: { item: localItem } });
    await flushPromises();

    expect(wrapper.find('.notes-title').text()).toBe('Untitled note');
    expect(wrapper.find('.notes-title--placeholder').exists()).toBe(true);
  });

  it('loads My Notes when switching between meetings that keep My Notes selected', async () => {
    notesCanEdit.mockReturnValue(true);
    getMeetingDetail.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve(detail({ id: meeting.id, title: meeting.title, isLocal: true }))
    );
    loadNote.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve({ content: `note ${meeting.id}`, title: '' })
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
    loadNote.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve({ content: `note ${meeting.id}`, title: '' })
    );

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

    expect(saveNote).toHaveBeenCalledWith(first, { content: 'note a', title: '' });
    expect(wrapper.text()).toContain('Second');
    expect(wrapper.text()).not.toContain('First');
  });

  it('shows the Share button for an Ariso host and opens the popover', async () => {
    getMeetingDetail.mockResolvedValue(
      detail({ isLocal: false, participants: [{ role: 'host', self: true }] })
    );
    const wrapper = mount(MeetingDetailView, {
      props: { item },
      global: { stubs: { ShareMeetingPopover: true } },
    });
    await flushPromises();

    expect(wrapper.find('.btn-share').exists()).toBe(true);
    expect(wrapper.findComponent({ name: 'ShareMeetingPopover' }).exists()).toBe(false);

    await wrapper.find('.btn-share').trigger('click');
    await flushPromises();

    expect(wrapper.findComponent({ name: 'ShareMeetingPopover' }).exists()).toBe(true);
  });

  it('shows the Share button for local recordings and shares natively', async () => {
    loadNote.mockResolvedValue({ content: '', title: '' });
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true, note: 'AI body' }));
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    expect(wrapper.find('.btn-share').exists()).toBe(true);

    await wrapper.find('.btn-share').trigger('click');
    await flushPromises();

    expect(shareTextNative).toHaveBeenCalled();
  });

  it('hides the Share button for an Ariso non-participant', async () => {
    getMeetingDetail.mockResolvedValue(
      detail({ isLocal: false, participants: [{ role: 'host', self: false }] })
    );
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    expect(wrapper.find('.btn-share').exists()).toBe(false);
  });
});

describe('MeetingDetailView audio player', () => {
  beforeEach(() => {
    URL.createObjectURL = vi.fn(() => 'blob:test');
    URL.revokeObjectURL = vi.fn();
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined);
  });

  it('shows the audio player in the Transcript tab for an Ariso meeting', async () => {
    // Transcript is the only content, so it is the active tab on mount.
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    expect(wrapper.find('.card-audio .play-btn').exists()).toBe(true);
  });

  it('renders the audio player only once the Transcript tab is opened', async () => {
    // Digest makes AI Notes the default tab; the player lives behind Transcript.
    const wrapper = await mountWith(detail({ digest: 'A quick digest', hasTranscript: true }));
    expect(wrapper.find('.card-audio').exists()).toBe(false);

    const transcriptTab = wrapper.findAll('.seg-btn').find((b) => b.text() === 'Transcript');
    await transcriptTab!.trigger('click');
    await flushPromises();

    expect(wrapper.find('.card-audio .play-btn').exists()).toBe(true);
  });

  it('does not show the audio player for a local recording, even in the Transcript tab', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi', hasTranscript: true }));
    const transcriptTab = wrapper.findAll('.seg-btn').find((b) => b.text() === 'Transcript');
    await transcriptTab!.trigger('click');
    await flushPromises();
    expect(wrapper.find('.card-audio').exists()).toBe(false);
  });

  it('clicking Play fetches audio through the backend that loaded the detail', async () => {
    getMeetingAudio.mockResolvedValue(new ArrayBuffer(4));
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    await wrapper.find('.card-audio .play-btn').trigger('click');
    await flushPromises();
    expect(getMeetingAudio).toHaveBeenCalledWith(item);
    expect(wrapper.find('.card-audio audio').exists()).toBe(true);
  });

  it('shows No audio when the meeting has no recording', async () => {
    getMeetingAudio.mockResolvedValue(null);
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    await wrapper.find('.card-audio .play-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.card-audio .play-btn').text()).toContain('No audio');
  });
});

describe('MeetingDetailView AI Assessment tab', () => {
  const tabByLabel = (wrapper: ReturnType<typeof mount>, label: string) =>
    wrapper.findAll('.seg-btn').find((b) => b.text() === label);

  it('shows an "AI Assessment" tab last when the meeting has a score', async () => {
    const wrapper = await mountWith(
      detail({ digest: 'A quick digest', score: 5, rationale: 'Great focus', recommendation: 'Keep it up' })
    );

    const labels = wrapper.findAll('.seg-btn').map((b) => b.text());
    expect(labels).toContain('AI Assessment');
    expect(labels[labels.length - 1]).toBe('AI Assessment');
  });

  it('renders the assessment in its own tab, not in AI Notes', async () => {
    const wrapper = await mountWith(
      detail({ digest: 'A quick digest', score: 4, rationale: 'Solid', recommendation: 'Tighten the agenda' })
    );

    // Defaults to AI Notes (digest present); the assessment lives behind its own tab.
    expect(tabByLabel(wrapper, 'AI Notes')!.classes()).toContain('seg-btn--active');
    expect(tabByLabel(wrapper, 'AI Assessment')!.classes()).not.toContain('seg-btn--active');

    await tabByLabel(wrapper, 'AI Assessment')!.trigger('click');
    await flushPromises();

    const circle = wrapper.find('.score-circle');
    expect(circle.isVisible()).toBe(true);
    expect(circle.text()).toBe('4');
    expect(wrapper.text()).toContain('Tighten the agenda');
  });

  it('shows the tab for a coaching-only assessment (no score)', async () => {
    const wrapper = await mountWith(
      detail({ digest: 'A quick digest', coaching: { strengths: ['Clear ask'] } })
    );
    expect(tabByLabel(wrapper, 'AI Assessment')).toBeTruthy();
  });

  it('hides the tab when the meeting has no score or coaching', async () => {
    const wrapper = await mountWith(detail({ digest: 'A quick digest' }));
    expect(tabByLabel(wrapper, 'AI Assessment')).toBeUndefined();
  });

  it('opens the assessment tab by default when it is the only content', async () => {
    const wrapper = await mountWith(detail({ score: 3, rationale: 'Mixed' }));
    expect(wrapper.find('.score-circle').isVisible()).toBe(true);
    expect(wrapper.find('.score-circle').text()).toBe('3');
  });
});

describe('MeetingDetailView local generation progress', () => {
  const localItem: MeetingListItem = {
    id: '7',
    title: 'Rec',
    timestamp: '2026-06-02T10:00:00Z',
    files: { hasAudio: true, hasNote: false, hasTranscript: false },
  };

  async function mountLocal(d: MeetingDetail) {
    getMeetingDetail.mockResolvedValue(d);
    const wrapper = mount(MeetingDetailView, { props: { item: localItem } });
    await flushPromises();
    return wrapper;
  }

  it('shows AI Notes + Transcript buttons (disabled) while transcribing, with a "Generating Transcript" chip', async () => {
    recordingStatus.mockResolvedValue({
      status: 'transcribing', hasTranscript: false, hasNote: false, notesStatus: 'pending',
    });
    const wrapper = await mountLocal(detail({ isLocal: true }));

    const labels = wrapper.findAll('.seg-btn').map((b) => b.text());
    expect(labels).toContain('AI Notes');
    expect(labels).toContain('Transcript');

    const aiNotes = wrapper.findAll('.seg-btn').find((b) => b.text() === 'AI Notes')!;
    const transcript = wrapper.findAll('.seg-btn').find((b) => b.text() === 'Transcript')!;
    expect(aiNotes.attributes('disabled')).toBeDefined();
    expect(transcript.attributes('disabled')).toBeDefined();

    expect(wrapper.find('.tab-status-label').text()).toBe('Generating Transcript');
    expect(wrapper.find('.tab-status .spinner').exists()).toBe(true);
    expect(wrapper.find('.tab-retry').exists()).toBe(false);
  });

  it('shows "Generating AI Notes" with the Transcript tab enabled once the transcript is ready', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: false, notesStatus: 'pending',
    });
    readRecordingFile.mockResolvedValue('# Transcript\nhi');
    const wrapper = await mountLocal(detail({ isLocal: true }));
    await flushPromises();

    const transcript = wrapper.findAll('.seg-btn').find((b) => b.text() === 'Transcript')!;
    expect(transcript.attributes('disabled')).toBeUndefined();
    const aiNotes = wrapper.findAll('.seg-btn').find((b) => b.text() === 'AI Notes')!;
    expect(aiNotes.attributes('disabled')).toBeDefined();
    expect(wrapper.find('.tab-status-label').text()).toBe('Generating AI Notes');
  });

  it('shows a Retry button on AI-notes failure and calls retryNotes', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: false, notesStatus: 'failed',
    });
    const wrapper = await mountLocal(detail({ isLocal: true, hasTranscript: true }));
    await flushPromises();

    expect(wrapper.find('.tab-status-label').text()).toBe('AI Notes failed');
    const retry = wrapper.find('.tab-retry');
    expect(retry.exists()).toBe(true);

    await retry.trigger('click');
    await flushPromises();
    expect(retryNotes).toHaveBeenCalledWith('7');
  });

  it('shows a Retry button on transcript failure and calls retryTranscription', async () => {
    recordingStatus.mockResolvedValue({
      status: 'failed', hasTranscript: false, hasNote: false, notesStatus: 'pending',
    });
    const wrapper = await mountLocal(detail({ isLocal: true }));
    await flushPromises();

    expect(wrapper.find('.tab-status-label').text()).toBe('Transcript failed');
    await wrapper.find('.tab-retry').trigger('click');
    await flushPromises();
    expect(retryTranscription).toHaveBeenCalledWith('7');
  });

  it('hides the chip and enables both tabs once notes are ready', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: true, notesStatus: 'ready',
    });
    readRecordingFile.mockResolvedValue('AI body');
    const wrapper = await mountLocal(detail({ isLocal: true, note: 'AI body', hasTranscript: true }));
    await flushPromises();

    expect(wrapper.find('.tab-status').exists()).toBe(false);
    const aiNotes = wrapper.findAll('.seg-btn').find((b) => b.text() === 'AI Notes')!;
    expect(aiNotes.attributes('disabled')).toBeUndefined();
  });

  it('shows a Regenerate notes button when local AI Notes are ready and clicking it calls retryNotes', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: true, notesStatus: 'ready',
    });
    readRecordingFile.mockResolvedValue('AI body');
    const wrapper = await mountLocal(detail({ isLocal: true, note: 'AI body', hasTranscript: true }));
    await flushPromises();

    // AI Notes is the default active tab (note present), so the button shows.
    const regen = wrapper.find('.tab-regen');
    expect(regen.exists()).toBe(true);
    expect(regen.text()).toContain('Regenerate notes');

    await regen.trigger('click');
    await flushPromises();
    expect(retryNotes).toHaveBeenCalledWith('7');
  });

  it('does not show the Regenerate notes button when no AI note exists yet', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: false, notesStatus: 'pending',
    });
    const wrapper = await mountLocal(detail({ isLocal: true, hasTranscript: true }));
    await flushPromises();
    expect(wrapper.find('.tab-regen').exists()).toBe(false);
  });

  it('does not show the Regenerate notes button for an Ariso meeting', async () => {
    const wrapper = await mountLocal(detail({ isLocal: false, digest: 'A digest' }));
    await flushPromises();
    expect(wrapper.find('.tab-regen').exists()).toBe(false);
  });

  it('hides the Regenerate notes button while notes are regenerating, showing the chip instead', async () => {
    recordingStatus.mockResolvedValue({
      status: 'done', hasTranscript: true, hasNote: false, notesStatus: 'pending',
    });
    const wrapper = await mountLocal(detail({ isLocal: true, note: 'Old body', hasTranscript: true }));
    await flushPromises();
    // A note exists (old body) but notes are generating -> chip owns the row.
    expect(wrapper.find('.tab-status-label').text()).toBe('Generating AI Notes');
    expect(wrapper.find('.tab-regen').exists()).toBe(false);
  });
});
