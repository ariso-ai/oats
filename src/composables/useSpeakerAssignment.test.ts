// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { computed, ref } from 'vue';

const request = vi.fn();
const fetchSpeakerAudio = vi.fn();

vi.mock('../tauri', () => ({
  api: {
    request: (...a: unknown[]) => request(...a),
    fetchSpeakerAudio: (...a: unknown[]) => fetchSpeakerAudio(...a),
  },
}));

import { useSpeakerAssignment } from './useSpeakerAssignment';
import type { AudioSpeaker } from './useMeetingApi';
import type { MeetingParticipantInfo } from './useBackend';

function audioSpeaker(over: Partial<AudioSpeaker> = {}): AudioSpeaker {
  return {
    speaker_index: 0,
    auto_matched_profile_id: null,
    auto_match_confidence: null,
    auto_match_name: null,
    auto_match_org_user_mapping_id: null,
    auto_match_email: null,
    ...over,
  };
}

function setup(
  over: {
    participants?: MeetingParticipantInfo[];
    audioSpeakers?: AudioSpeaker[];
    lines?: string[];
    meetingId?: string | null;
  } = {}
) {
  const participants = ref<MeetingParticipantInfo[]>(over.participants ?? []);
  const audioSpeakers = ref<AudioSpeaker[]>(over.audioSpeakers ?? []);
  const lines = ref<string[]>(over.lines ?? []);
  const meetingId = ref<string | null>(over.meetingId ?? '7');
  const speakers = useSpeakerAssignment({
    meetingId,
    participants,
    audioSpeakers,
    lines: computed(() => lines.value),
  });
  return { speakers, participants, audioSpeakers, lines, meetingId };
}

beforeEach(() => {
  vi.clearAllMocks();
  request.mockReset();
  request.mockResolvedValue({ status: 200, data: {} });
  fetchSpeakerAudio.mockReset();
});

describe('assignable speakers', () => {
  it('falls back to diarized indices before the transcript is loaded', () => {
    const { speakers } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 0 }), audioSpeaker({ speaker_index: 2 })],
    });
    expect(speakers.assignable.value).toEqual(['Speaker 1', 'Speaker 3']);
  });

  it('prefers the transcript once it is loaded', () => {
    const { speakers } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 0 })],
      lines: ['Speaker 2: hi', 'Ada Lovelace: hello'],
    });
    expect(speakers.assignable.value).toEqual(['Speaker 2', 'Ada Lovelace']);
  });

  it('is empty when the meeting has no diarized speakers and no transcript', () => {
    expect(setup().speakers.assignable.value).toEqual([]);
  });
});

describe('display names and the unresolved count', () => {
  it('resolves a speaker through its confirmed participant row', () => {
    const { speakers } = setup({
      participants: [{ participantId: 0, name: 'Ada Lovelace', manualConfirm: true }],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 }), audioSpeaker({ speaker_index: 1 })],
    });
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Ada Lovelace');
    expect(speakers.getSpeakerDisplayName('Speaker 2')).toBe('Speaker 2');
    expect(speakers.unresolved.value).toEqual(['Speaker 2']);
  });

  it('ignores a placeholder name, email, or display name on the row', () => {
    const { speakers } = setup({
      participants: [
        {
          participantId: 0,
          name: 'Speaker 1',
          displayName: 'Speaker 1',
          email: 'speaker-1@meeting-7.invalid',
        },
      ],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 })],
    });
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Speaker 1');
    expect(speakers.unresolved.value).toEqual(['Speaker 1']);
  });

  it('counts a confirmed-but-unnamed speaker as resolved', () => {
    const { speakers, participants } = setup({
      participants: [{ participantId: 0, manualConfirm: true }],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 })],
    });
    speakers.seedFromParticipants();
    expect(participants.value).toHaveLength(1);
    expect(speakers.unresolved.value).toEqual([]);
  });
});

describe('auto-match suggestions', () => {
  const ada = {
    auto_matched_profile_id: 5,
    auto_match_name: 'Ada Lovelace',
    auto_match_org_user_mapping_id: 'oum-1',
    auto_match_email: 'ada@example.com',
  };

  it('offers a strong match on its speaker', () => {
    const { speakers } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.92 })],
    });
    expect(speakers.getSpeakerAutoMatch('Speaker 1')).toMatchObject({
      name: 'Ada Lovelace',
      confidence: 0.92,
      outrankedBy: null,
    });
  });

  it('marks the weaker of two matches for one person as outranked', () => {
    const { speakers } = setup({
      audioSpeakers: [
        audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.97 }),
        audioSpeaker({ speaker_index: 2, ...ada, auto_match_confidence: 0.8 }),
      ],
    });
    expect(speakers.getSpeakerAutoMatch('Speaker 1')?.outrankedBy).toBeNull();
    expect(speakers.getSpeakerAutoMatch('Speaker 3')?.outrankedBy).toEqual({
      speakerLabel: 'Speaker 1',
      confidence: 0.97,
    });
  });

  it('has no suggestion for a speaker with no diarized row', () => {
    const { speakers } = setup({ lines: ['Ada Lovelace: hello'] });
    expect(speakers.getSpeakerAutoMatch('Ada Lovelace')).toBeNull();
  });
});

describe('assigning to an org member', () => {
  it('posts the org user and relabels the speaker', async () => {
    const { speakers, participants } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 1 })],
    });
    speakers.openAssignment('Speaker 2');
    await speakers.assignToMember({ id: 42, email: 'ada@example.com', name: 'Ada Lovelace' });

    expect(request).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/1/assign', {
      orgUserMappingId: 42,
    });
    expect(speakers.getSpeakerDisplayName('Speaker 2')).toBe('Ada Lovelace');
    expect(speakers.confirmed.value.has('Speaker 2')).toBe(true);
    expect(speakers.unresolved.value).toEqual([]);
    // No row existed for this diarized speaker, so one is added.
    expect(participants.value).toEqual([
      { id: 42, name: 'Ada Lovelace', participantId: 1, manualConfirm: true },
    ]);
  });

  it('detaches the person from whichever speaker held them before', async () => {
    // The server holds one person to one speaker index per meeting; the local
    // list has to follow or the same person shows up twice.
    const { speakers, participants } = setup({
      participants: [
        { participantId: 0, id: 42, name: 'Ada Lovelace', manualConfirm: true },
        { participantId: 1, name: 'Speaker 2' },
      ],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 }), audioSpeaker({ speaker_index: 1 })],
    });
    speakers.seedFromParticipants();
    speakers.openAssignment('Speaker 2');
    await speakers.assignToMember({ id: 42, email: 'ada@example.com', name: 'Ada Lovelace' });

    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Speaker 1');
    expect(speakers.getSpeakerDisplayName('Speaker 2')).toBe('Ada Lovelace');
    expect(speakers.confirmed.value.has('Speaker 1')).toBe(false);
    expect(participants.value[0]).toMatchObject({ name: 'Speaker 1', id: undefined });
  });

  it('leaves a plain calendar attendee alone when assigning that same person', async () => {
    // The host's own row carries no diarization index, so it holds no speaker
    // slot — the server never unlinks it, and blanking it here would strip a
    // real attendee's name off the meeting.
    const { speakers, participants } = setup({
      participants: [{ id: 42, name: 'Ada Lovelace', email: 'ada@example.com', role: 'host', self: true }],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 })],
    });
    speakers.openAssignment('Speaker 1');
    await speakers.assignToMember({ id: 42, email: 'ada@example.com', name: 'Ada Lovelace' });

    expect(participants.value[0]).toMatchObject({
      id: 42,
      name: 'Ada Lovelace',
      role: 'host',
      self: true,
    });
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Ada Lovelace');
  });

  it('matches the previous holder even when the two ids differ in JSON type', async () => {
    const { speakers, participants } = setup({
      participants: [{ participantId: 0, id: '42', name: 'Ada Lovelace', manualConfirm: true }],
      audioSpeakers: [audioSpeaker({ speaker_index: 0 }), audioSpeaker({ speaker_index: 1 })],
    });
    speakers.openAssignment('Speaker 2');
    await speakers.assignToMember({ id: 42, email: 'ada@example.com', name: 'Ada Lovelace' });

    expect(participants.value[0]).toMatchObject({ name: 'Speaker 1', id: undefined });
  });

  it('surfaces a failure and leaves the speaker unassigned', async () => {
    request.mockResolvedValue({ status: 403, data: { error: 'Not the host' } });
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    speakers.openAssignment('Speaker 1');
    await speakers.assignToMember({ id: 42, email: 'a@b.co', name: 'Ada' });

    expect(speakers.error.value).toBe('Not the host');
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Speaker 1');
    expect(speakers.unresolved.value).toEqual(['Speaker 1']);
  });
});

describe('assigning to typed text', () => {
  it('sends an email as an email', async () => {
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    speakers.openAssignment('Speaker 1');
    speakers.searchQuery.value = ' ada@example.com ';
    await speakers.assignToLabel();

    expect(request).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/0/assign', {
      email: 'ada@example.com',
    });
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('ada@example.com');
  });

  it('sends anything else as a display name', async () => {
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    speakers.openAssignment('Speaker 1');
    speakers.searchQuery.value = 'Ada Lovelace';
    await speakers.assignToLabel();

    expect(request).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/0/assign', {
      displayName: 'Ada Lovelace',
    });
  });

  it('does nothing on an empty query', async () => {
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    speakers.openAssignment('Speaker 1');
    speakers.searchQuery.value = '   ';
    await speakers.assignToLabel();
    expect(request).not.toHaveBeenCalled();
  });
});

describe('confirming a suggestion', () => {
  const ada = {
    auto_matched_profile_id: 5,
    auto_match_name: 'Ada Lovelace',
    auto_match_org_user_mapping_id: 'oum-1',
    auto_match_email: 'ada@example.com',
  };

  it('binds to the matched org user rather than degrading to a name', async () => {
    const { speakers } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.92 })],
    });
    await speakers.confirmSpeaker('Speaker 1');

    expect(request).toHaveBeenCalledWith('POST', '/meeting-notes/7/speakers/0/assign', {
      orgUserMappingId: 'oum-1',
    });
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Ada Lovelace');
  });

  it('refuses to confirm an outranked suggestion', async () => {
    // Accepting both in turn would hand Ada to whichever was clicked last.
    const { speakers } = setup({
      audioSpeakers: [
        audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.97 }),
        audioSpeaker({ speaker_index: 1, ...ada, auto_match_confidence: 0.8 }),
      ],
    });
    await speakers.confirmSpeaker('Speaker 2');

    expect(request).not.toHaveBeenCalled();
    expect(speakers.getSpeakerDisplayName('Speaker 2')).toBe('Speaker 2');
  });

  it('does nothing for a speaker with neither a label nor a suggestion', async () => {
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    await speakers.confirmSpeaker('Speaker 1');
    expect(request).not.toHaveBeenCalled();
  });

  it('rolls the optimistic label back when the request fails', async () => {
    request.mockResolvedValue({ status: 500, data: { error: 'boom' } });
    const { speakers } = setup({
      audioSpeakers: [audioSpeaker({ speaker_index: 0, ...ada, auto_match_confidence: 0.92 })],
    });
    await speakers.confirmSpeaker('Speaker 1');

    expect(speakers.error.value).toBe('boom');
    expect(speakers.getSpeakerDisplayName('Speaker 1')).toBe('Speaker 1');
    expect(speakers.confirmed.value.has('Speaker 1')).toBe(false);
  });
});

describe('member search', () => {
  it('debounces, queries, and stores the results', async () => {
    vi.useFakeTimers();
    try {
      request.mockResolvedValue({
        status: 200,
        data: { members: [{ id: 1, email: 'a@b.co', name: 'Ada' }] },
      });
      const { speakers } = setup();
      speakers.searchQuery.value = 'ad';
      speakers.debouncedSearch();
      speakers.debouncedSearch();
      expect(request).not.toHaveBeenCalled();

      await vi.runAllTimersAsync();
      expect(request).toHaveBeenCalledTimes(1);
      expect(request).toHaveBeenCalledWith(
        'GET',
        '/meeting-notes/7/speakers/search?q=ad'
      );
      expect(speakers.searchResults.value).toEqual([
        { id: 1, email: 'a@b.co', name: 'Ada' },
      ]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('skips a query shorter than two characters', async () => {
    vi.useFakeTimers();
    try {
      const { speakers } = setup();
      speakers.searchQuery.value = 'a';
      speakers.debouncedSearch();
      await vi.runAllTimersAsync();
      expect(request).not.toHaveBeenCalled();
      expect(speakers.searchResults.value).toEqual([]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('clears the previous results when a later search fails', async () => {
    // Stale members left on screen are worse than none: clicking one assigns
    // the speaker to whoever the *earlier* query matched.
    vi.useFakeTimers();
    try {
      request.mockResolvedValue({
        status: 200,
        data: { members: [{ id: 1, email: 'a@b.co', name: 'Ada' }] },
      });
      const { speakers } = setup();
      speakers.searchQuery.value = 'ada';
      speakers.debouncedSearch();
      await vi.runAllTimersAsync();
      expect(speakers.searchResults.value).toHaveLength(1);

      request.mockRejectedValue(new Error('offline'));
      speakers.searchQuery.value = 'adam';
      speakers.debouncedSearch();
      await vi.runAllTimersAsync();
      expect(speakers.searchResults.value).toEqual([]);
      expect(speakers.searching.value).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('reset cancels a queued search so it cannot answer for the next meeting', async () => {
    vi.useFakeTimers();
    try {
      const { speakers } = setup();
      speakers.searchQuery.value = 'ada';
      speakers.debouncedSearch();
      speakers.reset();
      await vi.runAllTimersAsync();
      expect(request).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('voice samples', () => {
  const play = vi.fn();
  let created: string[] = [];
  let revoked: string[] = [];

  beforeEach(() => {
    created = [];
    revoked = [];
    let n = 0;
    play.mockReset();
    play.mockResolvedValue(undefined);
    URL.createObjectURL = vi.fn(() => {
      const url = `blob:sample-${n++}`;
      created.push(url);
      return url;
    });
    URL.revokeObjectURL = vi.fn((url: string) => void revoked.push(url));
    vi.spyOn(window.HTMLMediaElement.prototype, 'play').mockImplementation(play);
    vi.spyOn(window.HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  });

  it('fetches by diarization index and plays', async () => {
    fetchSpeakerAudio.mockResolvedValue(new ArrayBuffer(8));
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 2 })] });

    await speakers.toggleVoiceSample(2);
    expect(fetchSpeakerAudio).toHaveBeenCalledWith('7', 2);
    expect(play).toHaveBeenCalled();
    expect(speakers.playingSpeakerIndex.value).toBe(2);
  });

  it('toggling the same speaker off stops playback and revokes the blob', async () => {
    fetchSpeakerAudio.mockResolvedValue(new ArrayBuffer(8));
    const { speakers } = setup();

    await speakers.toggleVoiceSample(0);
    await speakers.toggleVoiceSample(0);
    expect(speakers.playingSpeakerIndex.value).toBeNull();
    expect(revoked).toEqual(created);
  });

  it('starting another speaker revokes the first sample', async () => {
    fetchSpeakerAudio.mockResolvedValue(new ArrayBuffer(8));
    const { speakers } = setup();

    await speakers.toggleVoiceSample(0);
    await speakers.toggleVoiceSample(1);
    expect(revoked).toEqual([created[0]]);
    expect(speakers.playingSpeakerIndex.value).toBe(1);
  });

  it('a second Play mid-fetch wins, and the first neither plays nor leaks', async () => {
    // Both fetches are in flight before either resolves, so nothing is playing
    // yet and there is nothing for stopVoiceSample to tear down.
    const resolvers: Array<(bytes: ArrayBuffer) => void> = [];
    fetchSpeakerAudio.mockImplementation(
      () => new Promise<ArrayBuffer>((resolve) => resolvers.push(resolve))
    );
    const { speakers } = setup();

    const first = speakers.toggleVoiceSample(0);
    const second = speakers.toggleVoiceSample(1);
    resolvers[0](new ArrayBuffer(8));
    resolvers[1](new ArrayBuffer(8));
    await Promise.all([first, second]);

    expect(speakers.playingSpeakerIndex.value).toBe(1);
    // One Audio for the winner; the loser's blob is revoked, never played.
    expect(play).toHaveBeenCalledTimes(1);
    expect(created).toHaveLength(2);
    expect(revoked).toEqual([created[0]]);
  });

  it('a superseded fetch that fails does not stop the sample that replaced it', async () => {
    let failFirst: ((reason: unknown) => void) | undefined;
    fetchSpeakerAudio
      .mockImplementationOnce(() => new Promise((_, reject) => (failFirst = reject)))
      .mockResolvedValue(new ArrayBuffer(8));
    const { speakers } = setup();

    const first = speakers.toggleVoiceSample(0);
    await speakers.toggleVoiceSample(1);
    expect(speakers.playingSpeakerIndex.value).toBe(1);

    failFirst!('500: voice sample fetch failed');
    await first;
    expect(speakers.playingSpeakerIndex.value).toBe(1);
  });

  it('stopping mid-fetch keeps the sample from playing once it lands', async () => {
    let resolveBytes: ((bytes: ArrayBuffer) => void) | undefined;
    fetchSpeakerAudio.mockImplementation(
      () => new Promise<ArrayBuffer>((resolve) => (resolveBytes = resolve))
    );
    const { speakers } = setup();

    const pending = speakers.toggleVoiceSample(0);
    speakers.reset();
    resolveBytes!(new ArrayBuffer(8));
    await pending;

    expect(speakers.playingSpeakerIndex.value).toBeNull();
    expect(play).not.toHaveBeenCalled();
    expect(revoked).toEqual(created);
  });

  it('stays silent about a meeting with no stored sample', async () => {
    fetchSpeakerAudio.mockRejectedValue('404: voice sample fetch failed');
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { speakers } = setup();

    await speakers.toggleVoiceSample(0);
    expect(speakers.playingSpeakerIndex.value).toBeNull();
    expect(consoleError).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });
});

describe('reset', () => {
  it('drops every per-meeting bit of state', async () => {
    const { speakers } = setup({ audioSpeakers: [audioSpeaker({ speaker_index: 0 })] });
    speakers.open.value = true;
    speakers.openAssignment('Speaker 1');
    speakers.searchQuery.value = 'ada';
    await speakers.assignToLabel();
    expect(speakers.confirmed.value.size).toBe(1);

    speakers.reset();
    expect(speakers.open.value).toBe(false);
    expect(speakers.assigningSpeaker.value).toBeNull();
    expect(speakers.searchQuery.value).toBe('');
    expect(speakers.searchResults.value).toEqual([]);
    expect(speakers.assignments.value).toEqual({});
    expect(speakers.confirmed.value.size).toBe(0);
    expect(speakers.error.value).toBeNull();
  });
});
