// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper, type DOMWrapper } from '@vue/test-utils';
import { nextTick } from 'vue';
import type { MeetingDetail, MeetingListItem } from '../composables/useBackend';

const getMeetingDetail = vi.fn();
const getMeetingTranscript = vi.fn();
const renameMeeting = vi.fn();
const getMeetingAudio = vi.fn();
const deleteMeetingClip = vi.fn();
const getMeetingPrep = vi.fn();
const activeBackend = vi.fn();
const notesCanEdit = vi.fn(() => false);
const loadNote = vi.fn();
const saveNote = vi.fn();
const shareTextNative = vi.fn();
const loadPlatformCapabilities = vi.fn();
const recordingStatus = vi.fn();
const readRecordingFile = vi.fn();
const retryTranscription = vi.fn();
const retryNotes = vi.fn();
const apiRequest = vi.fn();
const fetchSpeakerAudio = vi.fn();
const pickMarkdownSavePath = vi.fn();
const copyRecordingFile = vi.fn();

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
vi.mock('../composables/usePlatformCapabilities', () => ({
  loadPlatformCapabilities: () => loadPlatformCapabilities(),
}));
vi.mock('../tauri', () => ({
  api: {
    request: (...a: unknown[]) => apiRequest(...a),
    fetchSpeakerAudio: (...a: unknown[]) => fetchSpeakerAudio(...a),
  },
  shareTextNative: (text: string, anchor: unknown) => shareTextNative(text, anchor),
  pickMarkdownSavePath: (name: string) => pickMarkdownSavePath(name),
  getDesktopConfig: () =>
    Promise.resolve({ webAppBaseUrl: 'https://app.test', pusherKey: '', pusherCluster: '' }),
  local: {
    recordingStatus: (id: string) => recordingStatus(id),
    readRecordingFile: (id: string, kind: string) => readRecordingFile(id, kind),
    retryTranscription: (id: string) => retryTranscription(id),
    retryNotes: (id: string) => retryNotes(id),
    copyRecordingFile: (id: string, kind: string, dest: string) =>
      copyRecordingFile(id, kind, dest),
  },
}));

import MeetingDetailView from './MeetingDetailView.vue';
import MeetingNotesEditor from './MeetingNotesEditor.vue';
import RecordingAudioPlayer from './RecordingAudioPlayer.vue';

function detail(over: Partial<MeetingDetail> = {}): MeetingDetail {
  return {
    id: '7',
    title: 'Old title',
    startAt: '2026-06-02T10:00:00Z',
    participants: [],
    audioSpeakers: [],
    actionItems: [],
    isLocal: false,
    audioClips: [],
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
  // clearAllMocks keeps queued `mockResolvedValueOnce` implementations, so an
  // unconsumed one would answer the next test's first detail load.
  getMeetingDetail.mockReset();
  renameMeeting.mockResolvedValue(undefined);
  getMeetingTranscript.mockResolvedValue(null);
  getMeetingAudio.mockResolvedValue(null);
  activeBackend.mockResolvedValue({
    getMeetingDetail: (i: MeetingListItem) => getMeetingDetail(i),
    getMeetingTranscript: (i: MeetingListItem) => getMeetingTranscript(i),
    renameMeeting: (...a: unknown[]) => renameMeeting(...a),
    getMeetingAudio: (...a: [MeetingListItem, string?]) => getMeetingAudio(...a),
    deleteMeetingClip: (...a: [MeetingListItem, string]) => deleteMeetingClip(...a),
    getMeetingPrep: (prepId: number) => getMeetingPrep(prepId),
  });
  notesCanEdit.mockReturnValue(false);
  loadNote.mockResolvedValue({ content: '', title: '' });
  saveNote.mockResolvedValue(undefined);
  loadPlatformCapabilities.mockResolvedValue({
    os: 'macos',
    localBackend: { supported: true, engine: 'swift-mlx' },
    systemAudio: { supported: true, settingsUrl: null },
    autoRecord: { supported: true },
    nativeShare: { supported: true },
    notificationSettingsUrl: null,
    microphoneSettingsUrl: null,
  });
  recordingStatus.mockResolvedValue({
    status: 'done',
    hasTranscript: false,
    hasNote: false,
    notesStatus: 'ready',
  });
  readRecordingFile.mockResolvedValue(null);
  pickMarkdownSavePath.mockResolvedValue('/Users/me/Desktop/export.md');
  copyRecordingFile.mockResolvedValue(undefined);
  retryTranscription.mockResolvedValue({ backend: 'local', id: '7', title: 'T', status: 'done' });
  retryNotes.mockResolvedValue(undefined);
  getMeetingPrep.mockResolvedValue(null);
  apiRequest.mockReset();
  apiRequest.mockResolvedValue({ status: 200, data: {} });
  fetchSpeakerAudio.mockReset();
  fetchSpeakerAudio.mockRejectedValue('404: voice sample fetch failed');
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
    expect(saveNote).not.toHaveBeenCalled();

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

  it('flushes a pending My Notes draft before reloading the detail pane', async () => {
    notesCanEdit.mockReturnValue(true);
    getMeetingDetail.mockImplementation((meeting: MeetingListItem) =>
      Promise.resolve(detail({ id: meeting.id, title: meeting.title, isLocal: true }))
    );
    loadNote.mockResolvedValue({ content: '', title: '' });

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

    wrapper.findComponent(MeetingNotesEditor).vm.$emit('update:modelValue', 'draft from call');
    await flushPromises();
    expect(saveNote).not.toHaveBeenCalled();

    await wrapper.setProps({ item: second });
    await flushPromises();

    expect(saveNote).toHaveBeenCalledWith(first, { content: 'draft from call', title: '' });
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

  it('hides local Share when the platform has no native share integration', async () => {
    loadPlatformCapabilities.mockResolvedValue({
      os: 'windows',
      localBackend: { supported: true, engine: 'cpp-sidecar' },
      systemAudio: { supported: false, settingsUrl: 'ms-settings:sound' },
      autoRecord: { supported: false },
      nativeShare: { supported: false },
      notificationSettingsUrl: 'ms-settings:notifications',
      microphoneSettingsUrl: 'ms-settings:privacy-microphone',
    });
    getMeetingDetail.mockResolvedValue(detail({ isLocal: true }));

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    expect(wrapper.find('.btn-share').exists()).toBe(false);
    expect(shareTextNative).not.toHaveBeenCalled();
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

describe('MeetingDetailView attendees dropdown', () => {
  const withParticipants = () =>
    detail({
      participants: [
        { name: 'Ada Lovelace', email: 'ada@example.com', role: 'host', self: true },
        { name: 'Alan Turing', email: 'alan@example.com', role: 'attendee' },
        { name: 'Grace Hopper', email: 'grace@example.com', role: 'attendee' },
        { name: 'Katherine Johnson', email: 'katherine@example.com', role: 'attendee' },
        { name: 'Edsger Dijkstra', email: 'edsger@example.com', role: 'attendee' },
      ],
    });

  it('opens a dropdown listing every attendee when the avatars are clicked', async () => {
    const wrapper = await mountWith(withParticipants());

    // Collapsed: no dropdown yet, and attendees past the visible avatars
    // (here the 5th, behind the "+1" chip) aren't shown by name anywhere.
    expect(wrapper.find('.attendees-menu').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('Edsger Dijkstra');

    await wrapper.find('.attendees-trigger').trigger('click');

    const menu = wrapper.find('.attendees-menu');
    expect(menu.exists()).toBe(true);
    // Every attendee is listed — including the one hidden behind "+1".
    expect(menu.findAll('.attendee-row')).toHaveLength(5);
    expect(menu.text()).toContain('Ada Lovelace');
    expect(menu.text()).toContain('Edsger Dijkstra');
    expect(menu.text()).toContain('ada@example.com');
    expect(menu.text()).toContain('host');
    // The current user is marked.
    expect(menu.text()).toContain('(me)');
  });

  it('closes the dropdown when the overlay is clicked', async () => {
    const wrapper = await mountWith(withParticipants());

    await wrapper.find('.attendees-trigger').trigger('click');
    expect(wrapper.find('.attendees-menu').exists()).toBe(true);

    await wrapper.find('.attendees-overlay').trigger('click');
    expect(wrapper.find('.attendees-menu').exists()).toBe(false);
  });

  it('closes the dropdown when Escape is pressed', async () => {
    const wrapper = await mountWith(withParticipants());

    await wrapper.find('.attendees-trigger').trigger('click');
    expect(wrapper.find('.attendees-menu').exists()).toBe(true);

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('.attendees-menu').exists()).toBe(false);
  });

  it('ignores Escape while the dropdown is closed', async () => {
    const wrapper = await mountWith(withParticipants());
    expect(wrapper.find('.attendees-menu').exists()).toBe(false);

    // No listener is registered while closed, so this is a no-op (and must
    // not throw).
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('.attendees-menu').exists()).toBe(false);
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

  it('shows the audio player for a local recording in the Transcript tab, like Ariso', async () => {
    getMeetingAudio.mockResolvedValue(new ArrayBuffer(4));
    const wrapper = await mountWith(detail({ isLocal: true, note: 'hi', hasTranscript: true }));
    const transcriptTab = wrapper.findAll('.seg-btn').find((b) => b.text() === 'Transcript');
    await transcriptTab!.trigger('click');
    await flushPromises();
    expect(wrapper.find('.card-audio .play-btn').exists()).toBe(true);

    // Play routes through the same backend.getMeetingAudio path as Ariso (local
    // reads read_recording_audio off disk).
    await wrapper.find('.card-audio .play-btn').trigger('click');
    await flushPromises();
    expect(getMeetingAudio).toHaveBeenCalledWith(item);
    expect(wrapper.find('.card-audio audio').exists()).toBe(true);
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

  it('renders one player per clip and filters transcript to the active clip', async () => {
    getMeetingTranscript.mockResolvedValue([
      { chunk_index: 0, start_ms: 0, content: 'from clip one', transcript_id: 'c1' },
      { chunk_index: 1, start_ms: 0, content: 'from clip two', transcript_id: 'c2' },
    ]);
    const wrapper = await mountWith(
      detail({
        hasTranscript: true,
        audioClips: [
          { transcript_id: 'c1', duration_ms: 60000, created_at: 't1', legacy: false },
          { transcript_id: 'c2', duration_ms: 30000, created_at: 't2', legacy: false },
        ],
      })
    );
    await flushPromises();

    expect(wrapper.findAllComponents(RecordingAudioPlayer)).toHaveLength(2);
    // Active defaults to the first clip -> only its chunk shows.
    expect(wrapper.text()).toContain('from clip one');
    expect(wrapper.text()).not.toContain('from clip two');
  });

  it('switches the displayed transcript when a different clip row is clicked', async () => {
    getMeetingTranscript.mockResolvedValue([
      { chunk_index: 0, start_ms: 0, content: 'from clip one', transcript_id: 'c1' },
      { chunk_index: 1, start_ms: 0, content: 'from clip two', transcript_id: 'c2' },
    ]);
    const wrapper = await mountWith(
      detail({
        hasTranscript: true,
        audioClips: [
          { transcript_id: 'c1', duration_ms: 60000, created_at: 't1', legacy: false },
          { transcript_id: 'c2', duration_ms: 30000, created_at: 't2', legacy: false },
        ],
      })
    );
    await flushPromises();

    const rows = wrapper.findAll('.clip-row');
    expect(rows).toHaveLength(2);
    expect(rows[0].classes()).toContain('clip-row--active');

    await rows[1].trigger('click');
    await flushPromises();

    expect(rows[1].classes()).toContain('clip-row--active');
    expect(wrapper.text()).toContain('from clip two');
    expect(wrapper.text()).not.toContain('from clip one');
  });

  it('activates a clip row on Space and reflects the selection via aria-pressed', async () => {
    getMeetingTranscript.mockResolvedValue([
      { chunk_index: 0, start_ms: 0, content: 'from clip one', transcript_id: 'c1' },
      { chunk_index: 1, start_ms: 0, content: 'from clip two', transcript_id: 'c2' },
    ]);
    const wrapper = await mountWith(
      detail({
        hasTranscript: true,
        audioClips: [
          { transcript_id: 'c1', duration_ms: 60000, created_at: 't1', legacy: false },
          { transcript_id: 'c2', duration_ms: 30000, created_at: 't2', legacy: false },
        ],
      })
    );
    await flushPromises();

    const rows = wrapper.findAll('.clip-row');
    // Default: first clip active, and aria-pressed mirrors the active row.
    expect(rows[0].attributes('aria-pressed')).toBe('true');
    expect(rows[1].attributes('aria-pressed')).toBe('false');

    // role="button" must activate on Space, not just Enter/click.
    await rows[1].trigger('keydown', { key: ' ' });
    await flushPromises();

    expect(rows[1].classes()).toContain('clip-row--active');
    expect(rows[1].attributes('aria-pressed')).toBe('true');
    expect(rows[0].attributes('aria-pressed')).toBe('false');
    expect(wrapper.text()).toContain('from clip two');
    expect(wrapper.text()).not.toContain('from clip one');
  });

  it("fetches a clip's audio via its transcript id when Play is clicked", async () => {
    getMeetingAudio.mockResolvedValue(new ArrayBuffer(4));
    const wrapper = await mountWith(
      detail({
        hasTranscript: true,
        audioClips: [
          { transcript_id: 'c1', duration_ms: 60000, created_at: 't1', legacy: false },
          { transcript_id: 'c2', duration_ms: 30000, created_at: 't2', legacy: false },
        ],
      })
    );
    await flushPromises();

    const rows = wrapper.findAll('.clip-row');
    await rows[1].find('.play-btn').trigger('click');
    await flushPromises();

    expect(getMeetingAudio).toHaveBeenCalledWith(item, 'c2');
  });

  it('keeps the single-player fallback and whole-meeting audio for a legacy single clip', async () => {
    getMeetingAudio.mockResolvedValue(new ArrayBuffer(4));
    const wrapper = await mountWith(
      detail({
        hasTranscript: true,
        audioClips: [{ transcript_id: 'legacy', duration_ms: null, created_at: 't0', legacy: true }],
      })
    );
    await flushPromises();

    expect(wrapper.find('.clip-row').exists()).toBe(false);
    expect(wrapper.findAllComponents(RecordingAudioPlayer)).toHaveLength(1);

    await wrapper.find('.card-audio .play-btn').trigger('click');
    await flushPromises();

    expect(getMeetingAudio).toHaveBeenCalledWith(item);
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

describe('MeetingDetailView per-clip delete', () => {
  const hostDetail = (over: Partial<MeetingDetail> = {}): MeetingDetail =>
    detail({
      hasTranscript: true,
      participants: [{ role: 'host', self: true }],
      ...over,
    });
  const twoClips = [
    { transcript_id: 'c1', duration_ms: 1000, created_at: 't1', legacy: false },
    { transcript_id: 'c2', duration_ms: 1000, created_at: 't2', legacy: false },
  ];

  it('shows a delete button per clip for a host with >1 clip, and deletes on confirm', async () => {
    getMeetingDetail
      .mockResolvedValueOnce(hostDetail({ audioClips: twoClips }))
      .mockResolvedValueOnce(
        hostDetail({
          audioClips: [{ transcript_id: 'c2', duration_ms: 1000, created_at: 't2', legacy: false }],
        })
      );
    getMeetingTranscript.mockResolvedValue([]);

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    const delButtons = wrapper.findAll('.clip-del-btn');
    expect(delButtons).toHaveLength(2);

    await delButtons[0].trigger('click');
    expect(deleteMeetingClip).not.toHaveBeenCalled();
    await wrapper.find('.danger-btn').trigger('click');
    await flushPromises();

    expect(deleteMeetingClip).toHaveBeenCalledWith(item, 'c1');
    // refetched -> one clip left -> per-clip delete no longer shown
    expect(wrapper.findAll('.clip-del-btn')).toHaveLength(0);
  });

  it('cancels without deleting', async () => {
    getMeetingDetail.mockResolvedValue(hostDetail({ audioClips: twoClips }));
    getMeetingTranscript.mockResolvedValue([]);

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    await wrapper.findAll('.clip-del-btn')[0].trigger('click');
    await wrapper.find('.secondary-btn').trigger('click');
    await flushPromises();

    expect(deleteMeetingClip).not.toHaveBeenCalled();
    expect(wrapper.findAll('.clip-del-btn')).toHaveLength(2);
  });

  it('shows no per-clip delete for a non-host, even with >1 clip', async () => {
    getMeetingDetail.mockResolvedValue(
      hostDetail({ participants: [{ role: 'host', self: false }], audioClips: twoClips })
    );
    getMeetingTranscript.mockResolvedValue([]);
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    expect(wrapper.findAll('.clip-del-btn')).toHaveLength(0);
  });

  it('shows no per-clip delete for a host with a single (or legacy) clip', async () => {
    getMeetingDetail.mockResolvedValue(
      hostDetail({ audioClips: [{ transcript_id: 'legacy', duration_ms: null, created_at: 't0', legacy: true }] })
    );
    getMeetingTranscript.mockResolvedValue([]);
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    expect(wrapper.findAll('.clip-del-btn')).toHaveLength(0);
  });

  it('surfaces an inline error and keeps the clip when deleteMeetingClip rejects', async () => {
    getMeetingDetail.mockResolvedValue(hostDetail({ audioClips: twoClips }));
    getMeetingTranscript.mockResolvedValue([]);
    deleteMeetingClip.mockRejectedValue(new Error('network down'));
    vi.spyOn(console, 'error').mockImplementation(() => {});

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    await wrapper.findAll('.clip-del-btn')[0].trigger('click');
    await wrapper.find('.danger-btn').trigger('click');
    await flushPromises();

    expect(deleteMeetingClip).toHaveBeenCalledWith(item, 'c1');
    // The dialog closed but the clip was NOT removed, and the failure is surfaced.
    expect(wrapper.findAll('.clip-del-btn')).toHaveLength(2);
    expect(wrapper.find('.clip-delete-error').exists()).toBe(true);
    expect(wrapper.find('.clip-delete-error').text()).toContain('Could not delete this recording');
  });

  it('preserves the active clip and its transcript when a non-active clip is deleted', async () => {
    getMeetingTranscript.mockResolvedValue([
      { chunk_index: 0, start_ms: 0, content: 'from clip one', transcript_id: 'c1' },
      { chunk_index: 1, start_ms: 0, content: 'from clip two', transcript_id: 'c2' },
    ]);
    getMeetingDetail
      .mockResolvedValueOnce(hostDetail({ audioClips: twoClips }))
      .mockResolvedValueOnce(
        hostDetail({
          audioClips: [{ transcript_id: 'c2', duration_ms: 1000, created_at: 't2', legacy: false }],
        })
      );

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    // Switch active clip to c2, then delete c1 (the non-active clip).
    const rows = wrapper.findAll('.clip-row');
    await rows[1].trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('from clip two');

    await wrapper.findAll('.clip-del-btn')[0].trigger('click');
    await wrapper.find('.danger-btn').trigger('click');
    await flushPromises();

    expect(deleteMeetingClip).toHaveBeenCalledWith(item, 'c1');
    // c2 stays active/showing, rather than snapping back to the (now sole) first clip.
    expect(wrapper.text()).toContain('from clip two');
    expect(wrapper.text()).not.toContain('from clip one');
  });
});

describe('MeetingDetailView meeting prep', () => {
  const tabLabels = (wrapper: VueWrapper): string[] =>
    wrapper.findAll('.seg-btn').map((b) => b.text());
  const prepTab = (wrapper: VueWrapper): DOMWrapper<Element> =>
    wrapper.findAll('.seg-btn').filter((b) => b.text() === 'Prep')[0];

  it('shows no Prep tab when the meeting has no prepId', async () => {
    const wrapper = await mountWith(detail({ digest: 'D' }));
    expect(tabLabels(wrapper)).not.toContain('Prep');
  });

  it('places the Prep tab after My Notes', async () => {
    const wrapper = await mountWith(
      detail({ prepId: 4339, digest: 'D', hasIndividualNote: true, hasTranscript: true })
    );
    expect(tabLabels(wrapper)).toEqual(['AI Notes', 'Transcript', 'My Notes', 'Prep']);
  });

  it('the Prep tab fetches once and renders the markdown', async () => {
    getMeetingPrep.mockResolvedValue({ content: '## Open items', meetingId: '45565' });
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));

    await prepTab(wrapper).trigger('click');
    await flushPromises();

    expect(getMeetingPrep).toHaveBeenCalledTimes(1);
    expect(getMeetingPrep).toHaveBeenCalledWith(4339);
    expect(wrapper.find('.prep-pane').text()).toContain('Open items');
    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');

    // Away and back: the cached prep is reused (no second fetch).
    await wrapper.find('.seg-btn').trigger('click'); // "AI Notes"
    expect(wrapper.find('.prep-pane').exists()).toBe(false);
    await prepTab(wrapper).trigger('click');
    await flushPromises();
    expect(getMeetingPrep).toHaveBeenCalledTimes(1);
  });

  it('clicking another tab leaves the prep pane and activates that tab', async () => {
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '1' });
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));
    await prepTab(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.find('.prep-pane').exists()).toBe(true);

    await wrapper.find('.seg-btn').trigger('click'); // "AI Notes"
    expect(wrapper.find('.prep-pane').exists()).toBe(false);
    expect(wrapper.find('.seg-btn--active').text()).toBe('AI Notes');
  });

  it('openPrepTab activates the Prep tab (the notification click target)', async () => {
    getMeetingPrep.mockResolvedValue({ content: '## From the notification', meetingId: '1' });
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));
    expect(wrapper.find('.seg-btn--active').text()).toBe('AI Notes');

    (wrapper.vm as unknown as { openPrepTab: () => void }).openPrepTab();
    await flushPromises();

    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');
    expect(wrapper.find('.prep-pane').text()).toContain('From the notification');
  });

  // The Library selects the prep's row and asks for the Prep tab one tick
  // later — that request races the detail load the selection just started.
  async function selectThenOpenPrep(
    wrapper: VueWrapper,
    next: MeetingListItem,
    requestedId: string
  ): Promise<void> {
    void wrapper.setProps({ item: next });
    await nextTick();
    (wrapper.vm as unknown as { openPrepTab: (id?: string) => void }).openPrepTab(requestedId);
    await flushPromises();
  }

  it('opens Prep on a meeting whose detail is still loading', async () => {
    // The previously loaded meeting carries a prep of its own, so its stale
    // detail must not answer a request meant for the incoming meeting.
    getMeetingDetail.mockResolvedValueOnce(detail({ id: '7', prepId: 11, digest: 'D' }));
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();
    expect(wrapper.find('.seg-btn--active').text()).toBe('AI Notes');

    getMeetingPrep.mockResolvedValue({ content: '## Next prep', meetingId: '8' });
    getMeetingDetail.mockResolvedValueOnce(detail({ id: '8', prepId: 4339, digest: 'D' }));
    await selectThenOpenPrep(
      wrapper,
      { id: '8', title: 'Next', timestamp: '2026-06-03T10:00:00Z' },
      '8'
    );

    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');
    expect(wrapper.find('.prep-pane').text()).toContain('Next prep');
  });

  it('opens Prep even when a pending note save delays the meeting switch', async () => {
    // `load` flushes a dirty draft before resetting its state, so the reset can
    // land *after* the Library has already asked for the Prep tab.
    notesCanEdit.mockReturnValue(true);
    getMeetingDetail.mockImplementation((m: MeetingListItem) =>
      Promise.resolve(detail({ id: m.id, prepId: m.id === '8' ? 4339 : 11, digest: 'D' }))
    );
    getMeetingPrep.mockResolvedValue({ content: '## Next prep', meetingId: '8' });

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();
    // Autosave only arms once the personal note has loaded, i.e. after its tab
    // has been opened — that is what leaves a dirty draft behind.
    await wrapper.findAll('.seg-btn').filter((b) => b.text() === 'My Notes')[0].trigger('click');
    await flushPromises();
    wrapper.findComponent(MeetingNotesEditor).vm.$emit('update:modelValue', 'draft');
    await flushPromises();

    let resolveSave: (() => void) | null = null;
    saveNote.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        })
    );

    void wrapper.setProps({ item: { id: '8', title: 'Next', timestamp: '2026-06-03T10:00:00Z' } });
    await nextTick();
    (wrapper.vm as unknown as { openPrepTab: (id?: string) => void }).openPrepTab('8');
    resolveSave?.();
    await flushPromises();

    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');
  });

  it('stays on Prep when the same meeting reloads under it', async () => {
    // A list refresh swaps the selected row for the freshly-fetched one, which
    // reloads the detail. The prep request must survive that reload.
    getMeetingDetail.mockImplementation((m: MeetingListItem) =>
      Promise.resolve(detail({ id: m.id, prepId: 4339, digest: 'D', hasIndividualNote: true }))
    );
    getMeetingPrep.mockResolvedValue({ content: '## Talking points', meetingId: '7' });

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();
    (wrapper.vm as unknown as { openPrepTab: (id?: string) => void }).openPrepTab('7');
    await flushPromises();
    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');

    // Same meeting, fresh row object (a slightly different timestamp is enough
    // to retrigger the load).
    await wrapper.setProps({ item: { ...item, timestamp: '2026-06-02T10:00:01Z' } });
    await flushPromises();

    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');
  });

  it('drops the prep request once the user picks another tab', async () => {
    getMeetingDetail.mockImplementation((m: MeetingListItem) =>
      Promise.resolve(detail({ id: m.id, prepId: 4339, digest: 'D', hasIndividualNote: true }))
    );
    getMeetingPrep.mockResolvedValue({ content: '## Talking points', meetingId: '7' });

    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();
    (wrapper.vm as unknown as { openPrepTab: (id?: string) => void }).openPrepTab('7');
    await flushPromises();

    await wrapper.findAll('.seg-btn').filter((b) => b.text() === 'My Notes')[0].trigger('click');
    await flushPromises();
    await wrapper.setProps({ item: { ...item, timestamp: '2026-06-02T10:00:01Z' } });
    await flushPromises();

    // The user's choice outranks the notification — a reload must not yank them
    // back to Prep.
    expect(wrapper.find('.seg-btn--active').text()).toBe('AI Notes');
  });

  it('ignores a prep request aimed at a meeting that is no longer selected', async () => {
    getMeetingDetail.mockResolvedValueOnce(detail({ id: '7', prepId: 11, digest: 'D' }));
    const wrapper = mount(MeetingDetailView, { props: { item } });
    await flushPromises();

    getMeetingDetail.mockResolvedValueOnce(detail({ id: '8', prepId: 4339, digest: 'D' }));
    await selectThenOpenPrep(
      wrapper,
      { id: '8', title: 'Next', timestamp: '2026-06-03T10:00:00Z' },
      '7'
    );

    expect(wrapper.find('.seg-btn--active').text()).toBe('AI Notes');
  });

  it('shows an empty state when the prep API resolves null', async () => {
    getMeetingPrep.mockResolvedValue(null);
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));
    await prepTab(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.find('.prep-pane').text()).toContain('No prep available.');
  });

  it('shows an empty state when the prep has no content', async () => {
    getMeetingPrep.mockResolvedValue({ content: null, meetingId: '1' });
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));
    await prepTab(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.find('.prep-pane').text()).toContain('No prep available.');
  });

  it('shows an error state when the prep fetch fails', async () => {
    getMeetingPrep.mockRejectedValue(new Error('boom'));
    const wrapper = await mountWith(detail({ prepId: 4339, digest: 'D' }));
    await prepTab(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.find('.prep-pane').text()).toContain('Could not load the meeting prep.');
  });
});

describe('MeetingDetailView canceled meetings', () => {
  it('marks a canceled meeting with a chip', async () => {
    const wrapper = await mountWith(detail({ canceled: true }));
    expect(wrapper.find('.canceled-tag').exists()).toBe(true);
    expect(wrapper.find('.canceled-tag').text()).toContain('Canceled');
  });

  it('hides the "Ari will join" tag on a canceled meeting', async () => {
    const wrapper = await mountWith(detail({ canceled: true, autoJoinScheduled: true }));
    expect(wrapper.find('.ari-tag').exists()).toBe(false);
    expect(wrapper.find('.canceled-tag').exists()).toBe(true);
  });

  it('still shows the "Ari will join" tag on a live meeting', async () => {
    const wrapper = await mountWith(detail({ autoJoinScheduled: true }));
    expect(wrapper.find('.ari-tag').exists()).toBe(true);
    expect(wrapper.find('.canceled-tag').exists()).toBe(false);
  });

  it('shows no canceled chip for a normal meeting', async () => {
    const wrapper = await mountWith(detail());
    expect(wrapper.find('.canceled-tag').exists()).toBe(false);
  });
});

describe('MeetingDetailView Ari chip', () => {
  const NOW = new Date('2026-06-16T12:00:00Z');
  // Started at 11:45, ends at 12:30 — running as of NOW.
  const running = {
    autoJoinScheduled: true,
    startAt: '2026-06-16T11:45:00Z',
    endAt: '2026-06-16T12:30:00Z',
  };

  async function mountAt(d: MeetingDetail, now = NOW) {
    getMeetingDetail.mockResolvedValue(d);
    const wrapper = mount(MeetingDetailView, { props: { item, now } });
    await flushPromises();
    return wrapper;
  }

  it('says Ari has joined for a running meeting whose status is joined', async () => {
    const wrapper = await mountAt(detail({ ...running, arisoStatus: 'joined' }));
    expect(wrapper.find('.ari-tag').text()).toContain('Ari has joined');
  });

  it('keeps the promise while the meeting is running but Ari has not joined', async () => {
    const wrapper = await mountAt(detail({ ...running, arisoStatus: 'joining' }));
    expect(wrapper.find('.ari-tag').text()).toContain('Ari will join');
  });

  it('drops the chip once the meeting has ended', async () => {
    const wrapper = await mountAt(
      detail({
        autoJoinScheduled: true,
        arisoStatus: 'joined',
        startAt: '2026-06-16T10:00:00Z',
        endAt: '2026-06-16T11:00:00Z',
      })
    );
    expect(wrapper.find('.ari-tag').exists()).toBe(false);
  });

  it('drops the chip once the meeting is done', async () => {
    const wrapper = await mountAt(detail({ ...running, arisoStatus: 'done' }));
    expect(wrapper.find('.ari-tag').exists()).toBe(false);
  });
});

describe('MeetingDetailView speaker assignment', () => {
  const audioSpeaker = (over: Record<string, unknown> = {}) => ({
    speaker_index: 0,
    auto_matched_profile_id: null,
    auto_match_confidence: null,
    auto_match_name: null,
    auto_match_org_user_mapping_id: null,
    auto_match_email: null,
    ...over,
  });

  const speakerDetail = (over: Partial<MeetingDetail> = {}): MeetingDetail =>
    detail({
      participants: [{ role: 'host', self: true }],
      hasAudioRecording: true,
      audioSpeakers: [audioSpeaker({ speaker_index: 0 }), audioSpeaker({ speaker_index: 1 })],
      ...over,
    });

  // The panel teleports into <body>, so wrapper.find() cannot see it.
  const q = (selector: string) => document.querySelector(selector) as HTMLElement | null;
  const qAll = (selector: string) =>
    Array.from(document.querySelectorAll(selector)) as HTMLElement[];

  let wrapper: VueWrapper | null = null;

  async function openPanel(d: MeetingDetail) {
    getMeetingDetail.mockResolvedValue(d);
    wrapper = mount(MeetingDetailView, { props: { item }, attachTo: document.body });
    await flushPromises();
    await wrapper.find('.speakers-trigger').trigger('click');
    await flushPromises();
    return wrapper;
  }

  afterEach(() => {
    wrapper?.unmount();
    wrapper = null;
    document.body.innerHTML = '';
  });

  it('shows the chip for a host on a cloud meeting with recorded audio', async () => {
    const w = await mountWith(speakerDetail());
    expect(w.find('.speakers-trigger').exists()).toBe(true);
    expect(w.find('.speakers-label').text()).toBe('2 unassigned');
  });

  it('counts down as speakers get resolved, then reads as a total', async () => {
    const w = await mountWith(
      speakerDetail({
        participants: [
          { role: 'host', self: true },
          { participantId: 0, name: 'Ada Lovelace', manualConfirm: true },
        ],
      })
    );
    expect(w.find('.speakers-label').text()).toBe('1 unassigned');
  });

  it('stays visible once every speaker is resolved — a confirm can be wrong', async () => {
    const w = await mountWith(
      speakerDetail({
        audioSpeakers: [audioSpeaker({ speaker_index: 0 })],
        participants: [
          { role: 'host', self: true },
          { participantId: 0, name: 'Ada Lovelace', manualConfirm: true },
        ],
      })
    );
    expect(w.find('.speakers-label').text()).toBe('1 Speaker');
  });

  it('hides the chip from a non-host', async () => {
    const w = await mountWith(speakerDetail({ participants: [{ role: 'attendee', self: true }] }));
    expect(w.find('.speakers-trigger').exists()).toBe(false);
  });

  it('hides the chip when the meeting has no recorded audio', async () => {
    const w = await mountWith(speakerDetail({ hasAudioRecording: false }));
    expect(w.find('.speakers-trigger').exists()).toBe(false);
  });

  it('hides the chip when nothing was diarized', async () => {
    const w = await mountWith(speakerDetail({ audioSpeakers: [] }));
    expect(w.find('.speakers-trigger').exists()).toBe(false);
  });

  it('hides the chip for a local recording', async () => {
    const w = await mountWith(
      speakerDetail({ isLocal: true, participants: [{ role: 'host', self: true }] })
    );
    expect(w.find('.speakers-trigger').exists()).toBe(false);
  });

  it('opens a row per diarized speaker without waiting on the transcript', async () => {
    await openPanel(speakerDetail());
    expect(getMeetingTranscript).not.toHaveBeenCalled();
    expect(qAll('.sp-row')).toHaveLength(2);
    expect(qAll('.sp-speaker').map((el) => el.textContent)).toEqual(['Speaker 1', 'Speaker 2']);
  });

  it('assigns a speaker to a teammate found by search', async () => {
    vi.useFakeTimers();
    try {
      await openPanel(speakerDetail());
      qAll('.sp-btn').find((b) => b.textContent?.trim() === 'Assign')!.click();
      await nextTick();

      const input = q('.sp-input') as HTMLInputElement;
      apiRequest.mockResolvedValue({
        status: 200,
        data: { members: [{ id: 42, email: 'ada@x.com', name: 'Ada Lovelace' }] },
      });
      input.value = 'ada';
      input.dispatchEvent(new Event('input'));
      await vi.runAllTimersAsync();
      await nextTick();

      expect(q('.sp-result-name')?.textContent).toContain('Ada Lovelace');
      apiRequest.mockResolvedValue({ status: 200, data: {} });
      q('.sp-result')!.click();
      await flushPromises();

      expect(apiRequest).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/0/assign', {
        orgUserMappingId: 42,
      });
      expect(q('.sp-name')?.textContent).toBe('Ada Lovelace');
      expect(wrapper!.find('.speakers-label').text()).toBe('1 unassigned');
    } finally {
      vi.useRealTimers();
    }
  });

  it('offers a one-click confirm on the strongest match only', async () => {
    const ada = {
      auto_matched_profile_id: 5,
      auto_match_name: 'Ada Lovelace',
      auto_match_org_user_mapping_id: 'oum-1',
      auto_match_email: 'ada@x.com',
    };
    await openPanel(
      speakerDetail({
        audioSpeakers: [
          audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.97 }),
          audioSpeaker({ speaker_index: 1, ...ada, auto_match_confidence: 0.8 }),
        ],
      })
    );

    const rows = qAll('.sp-row');
    expect(rows[0].textContent).toContain('97% match');
    expect(rows[0].querySelector('.sp-btn--confirm')).not.toBeNull();
    // The weaker one keeps its row and its score, but not the shortcut.
    expect(rows[1].textContent).toContain('80% match');
    expect(rows[1].querySelector('.sp-btn--confirm')).toBeNull();
    expect(rows[1].textContent).toContain('Speaker 1 is a stronger match');

    rows[0].querySelector<HTMLElement>('.sp-btn--confirm')!.click();
    await flushPromises();
    expect(apiRequest).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/0/assign', {
      orgUserMappingId: 'oum-1',
    });
  });

  it('plays a voice sample by diarization index', async () => {
    fetchSpeakerAudio.mockResolvedValue(new ArrayBuffer(8));
    URL.createObjectURL = vi.fn(() => 'blob:sample');
    URL.revokeObjectURL = vi.fn();
    vi.spyOn(window.HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined);
    vi.spyOn(window.HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});

    await openPanel(speakerDetail());
    qAll('.sp-play')[1].click();
    await flushPromises();

    expect(fetchSpeakerAudio).toHaveBeenCalledWith('7', 1);
    expect(qAll('.sp-play')[1].textContent?.trim()).toBe('Stop');
  });

  it('opening the attendees dropdown closes the speaker panel', async () => {
    const w = await openPanel(speakerDetail());
    expect(q('.sp-pop')).not.toBeNull();
    await w.find('.attendees-trigger').trigger('click');
    await nextTick();
    expect(q('.sp-pop')).toBeNull();
  });

  it('clicking the click-catcher closes the panel', async () => {
    await openPanel(speakerDetail());
    q('.sp-overlay')!.click();
    await nextTick();
    expect(q('.sp-pop')).toBeNull();
  });

  it('surfaces a failed assignment instead of showing a name that was never saved', async () => {
    await openPanel(speakerDetail());
    qAll('.sp-btn').find((b) => b.textContent?.trim() === 'Assign')!.click();
    await nextTick();

    const input = q('.sp-input') as HTMLInputElement;
    input.value = 'Ada Lovelace';
    input.dispatchEvent(new Event('input'));
    await nextTick();
    apiRequest.mockResolvedValue({ status: 403, data: { error: 'Only the host can do that' } });
    q('.sp-result')!.click();
    await flushPromises();

    expect(q('.sp-err')?.textContent).toContain('Only the host can do that');
    expect(q('.sp-name')).toBeNull();
    expect(wrapper!.find('.speakers-label').text()).toBe('2 unassigned');
  });
});

describe('MeetingDetailView transcript download', () => {
  it('does not offer download for an Ariso meeting', async () => {
    const wrapper = await mountWith(detail({ hasTranscript: true }));
    expect(wrapper.find('.btn-download').exists()).toBe(false);
  });

  it('disables the button with an explanatory tooltip until the transcript exists', async () => {
    const wrapper = await mountWith(detail({ isLocal: true, hasTranscript: false }));
    const btn = wrapper.find('.btn-download');
    expect(btn.exists()).toBe(true);
    expect(btn.attributes('disabled')).toBeDefined();
    expect(btn.attributes('title')).toBe('Transcript not ready yet');
  });

  it('copies the transcript to the picked path', async () => {
    const wrapper = await mountWith(
      detail({ id: 'rec-1', isLocal: true, hasTranscript: true, title: 'Weekly sync' })
    );
    const btn = wrapper.find('.btn-download');
    expect(btn.attributes('disabled')).toBeUndefined();

    await btn.trigger('click');
    await flushPromises();

    expect(pickMarkdownSavePath).toHaveBeenCalledWith('Weekly sync transcript - 2026-06-02.md');
    expect(copyRecordingFile).toHaveBeenCalledWith('rec-1', 'transcript', '/Users/me/Desktop/export.md');
    expect(wrapper.find('.transcript-download-error').exists()).toBe(false);
  });

  it('does nothing when the user cancels the save dialog', async () => {
    pickMarkdownSavePath.mockResolvedValue(null);
    const wrapper = await mountWith(detail({ isLocal: true, hasTranscript: true }));

    await wrapper.find('.btn-download').trigger('click');
    await flushPromises();

    expect(copyRecordingFile).not.toHaveBeenCalled();
    expect(wrapper.find('.transcript-download-error').exists()).toBe(false);
  });

  it('shows an inline error when the copy fails', async () => {
    copyRecordingFile.mockRejectedValue(new Error('Permission denied'));
    const wrapper = await mountWith(detail({ isLocal: true, hasTranscript: true }));

    await wrapper.find('.btn-download').trigger('click');
    await flushPromises();

    expect(wrapper.find('.transcript-download-error').text()).toContain(
      "Couldn't save the transcript: Permission denied"
    );
    // The failure is inline; the tab stays usable and the button re-enables.
    expect(wrapper.find('.btn-download').attributes('disabled')).toBeUndefined();
  });
});
