import { computed, ref, type ComputedRef, type Ref } from 'vue';
import { api } from '../tauri';
import {
  useMeetingApi,
  type AudioSpeaker,
  type SpeakerIdentity,
  type SpeakerSearchMember,
} from './useMeetingApi';
import type { MeetingParticipantInfo } from './useBackend';
import {
  rankAutoMatchesByIdentity,
  speakersInTranscript,
  toAutoMatchSuggestion,
  type AutoMatchSuggestion,
} from './speakerAutoMatch';

/** How a suggestion is presented when a stronger one already claims the person. */
export interface OutrankedBy {
  /** `Speaker N` label of the speaker that matched this person more strongly. */
  speakerLabel: string;
  confidence: number;
}

export interface SpeakerAutoMatch {
  name: string;
  confidence: number;
  orgUserMappingId: string | null;
  email: string | null;
  /**
   * Null when this is the strongest match for whoever it names. Otherwise the
   * speaker that beat it — the suggestion still shows (it is real evidence, and
   * diarization splitting one person across two voices is exactly when a host
   * wants to see both), but it is no longer offered as a one-click accept.
   */
  outrankedBy: OutrankedBy | null;
}

/** How long the search field sits idle before a query goes to the server. */
const SEARCH_DEBOUNCE_MS = 300;
/** Below this, a query is too broad to be worth a round trip. */
const MIN_SEARCH_LENGTH = 2;

/**
 * Speaker → person assignment for one Ariso meeting.
 *
 * Ariso-only by construction: it needs the server's voice matching and its org
 * directory. The caller gates on `!detail.isLocal`; nothing here is reachable
 * from the offline backend.
 *
 * `participants` is passed in as a ref rather than owned here because assigning
 * a speaker mutates the meeting's attendee list in place — the avatars in the
 * metadata band and the share popover all render off that same array.
 */
export function useSpeakerAssignment(deps: {
  /** Ariso meeting id; null while no meeting is loaded. */
  meetingId: Ref<string | null>;
  participants: Ref<MeetingParticipantInfo[]>;
  /** Diarized speakers for the meeting, keyed by `speaker_index`. Source of
   *  truth for auto-match suggestions, since `participants` holds only
   *  confirmed attendees. */
  audioSpeakers: Ref<AudioSpeaker[]>;
  /** Transcript lines, when the transcript has been loaded. Empty otherwise —
   *  the surface has to work before anyone opens the Transcript tab. */
  lines: ComputedRef<string[]>;
}) {
  const { meetingId, participants, audioSpeakers, lines } = deps;
  const meetingApi = useMeetingApi();

  const open = ref(false);
  /** Which speaker's inline "assign to…" field is showing, if any. */
  const assigningSpeaker = ref<string | null>(null);
  const searchQuery = ref('');
  const searchResults = ref<SpeakerSearchMember[]>([]);
  const searching = ref(false);
  /** Optimistic speaker → name labels applied since the meeting loaded. */
  const assignments = ref<Record<string, string>>({});
  /** Speakers the host has confirmed, by `Speaker N` label. */
  const confirmed = ref<Set<string>>(new Set());
  const error = ref<string | null>(null);

  // Voice-sample playback.
  const playingSpeakerIndex = ref<number | null>(null);
  let voiceSampleAudio: HTMLAudioElement | null = null;
  // Object URL backing the current sample, tracked so a manual stop (toggle
  // off, reset, meeting switch) can revoke it. `onended`/`onerror` only fire on
  // natural completion, so without this every start/stop cycle would retain the
  // audio blob for the window's lifetime.
  let voiceSampleUrl: string | null = null;

  let searchTimeout: ReturnType<typeof setTimeout> | null = null;

  const stopVoiceSample = (): void => {
    voiceSampleAudio?.pause();
    voiceSampleAudio = null;
    playingSpeakerIndex.value = null;
    if (voiceSampleUrl) {
      URL.revokeObjectURL(voiceSampleUrl);
      voiceSampleUrl = null;
    }
  };

  const closeAssignment = (): void => {
    assigningSpeaker.value = null;
    searchQuery.value = '';
    searchResults.value = [];
  };

  /** Clear every per-meeting bit of state. Call before loading another one. */
  const reset = (): void => {
    stopVoiceSample();
    closeAssignment();
    open.value = false;
    searching.value = false;
    assignments.value = {};
    confirmed.value = new Set();
    error.value = null;
    // Cancel a search queued right before the switch so it can't fire ~300ms
    // later and repopulate results against the previous meeting.
    if (searchTimeout) {
      clearTimeout(searchTimeout);
      searchTimeout = null;
    }
  };

  /** Seed `confirmed` from the participants the API reports as confirmed. */
  const seedFromParticipants = (): void => {
    confirmed.value = new Set(
      participants.value
        .filter((p) => p.manualConfirm && p.participantId != null)
        .map((p) => `Speaker ${p.participantId! + 1}`)
    );
    assignments.value = {};
  };

  const getSpeakerDisplayName = (speakerName: string): string => {
    // An assignment made in this session wins: it is newer than the payload
    // the participant list was built from.
    if (assignments.value[speakerName]) return assignments.value[speakerName];

    const match = speakerName.match(/^Speaker (\d+)$/);
    if (!match) return speakerName;
    const participantId = parseInt(match[1], 10) - 1;
    const participant = participants.value.find(
      (p) => p.participantId === participantId
    );
    if (!participant) return speakerName;

    // Prefer an explicit display name, then a real name, then an email — but
    // never a generic "Speaker N" echo or a synthetic placeholder address,
    // which would render as an assignment when nothing has been assigned.
    if (participant.displayName && !participant.displayName.startsWith('Speaker ')) {
      return participant.displayName;
    }
    if (participant.name && !participant.name.startsWith('Speaker ')) {
      return participant.name;
    }
    if (
      participant.email &&
      !participant.email.includes('@meeting-') &&
      !participant.email.startsWith('speaker-')
    ) {
      return participant.email;
    }
    return speakerName;
  };

  /** Unique speakers appearing in the transcript, in first-seen order. */
  const transcriptSpeakers = computed(() => speakersInTranscript(lines.value));

  /**
   * Speakers the host can assign.
   *
   * Prefers the transcript-derived list — it is authoritative, and a
   * bot-captured meeting labels its lines with real names that have no
   * diarized-speaker row at all — and falls back to the diarized indices from
   * the meeting payload so the chip and the panel work before the Transcript
   * tab has ever been opened, which is the common case.
   */
  const assignable = computed(() =>
    transcriptSpeakers.value.length > 0
      ? transcriptSpeakers.value
      : audioSpeakers.value.map((s) => `Speaker ${s.speaker_index + 1}`)
  );

  /** Speakers not yet mapped to a person — the actionable number on the chip. */
  const unresolved = computed(() =>
    assignable.value.filter(
      (s) => getSpeakerDisplayName(s) === s && !confirmed.value.has(s)
    )
  );

  const findParticipantForSpeaker = (
    speakerName: string
  ): MeetingParticipantInfo | undefined =>
    participants.value.find(
      (p) =>
        p.name === speakerName ||
        (p.participantId != null && `Speaker ${p.participantId + 1}` === speakerName)
    );

  /**
   * Diarization index used to address a speaker's voice sample and its assign
   * endpoint. Samples are stored per meeting by index, so one is playable
   * before the speaker has any participant row — the norm, since only confirmed
   * assignments are promoted to attendees. Prefer an existing row (a
   * bot-captured transcript labels speakers by real name) and otherwise recover
   * the index from the generic `Speaker N` label.
   */
  const getSpeakerIndex = (speakerName: string): number | null => {
    const participant = findParticipantForSpeaker(speakerName);
    if (participant?.participantId != null) return participant.participantId;
    const match = speakerName.match(/^Speaker (\d+)$/);
    return match ? parseInt(match[1], 10) - 1 : null;
  };

  // Every offerable suggestion in the meeting, and which of them a stronger
  // match for the same person already speaks for. Computed across all speakers
  // at once because that is the only level at which the question can be
  // answered — a suggestion on its own never looks wrong.
  const outrankedAutoMatches = computed(() =>
    rankAutoMatchesByIdentity(
      audioSpeakers.value
        .map(toAutoMatchSuggestion)
        .filter((s): s is AutoMatchSuggestion => s !== null)
    )
  );

  const getSpeakerAutoMatch = (speakerName: string): SpeakerAutoMatch | null => {
    const index = getSpeakerIndex(speakerName);
    if (index === null) return null;
    const speaker = audioSpeakers.value.find((s) => s.speaker_index === index);
    const suggestion = speaker ? toAutoMatchSuggestion(speaker) : null;
    if (!suggestion) return null;

    const winner = outrankedAutoMatches.value.get(suggestion.speakerIndex);
    return {
      name: suggestion.name,
      confidence: suggestion.confidence,
      orgUserMappingId: suggestion.orgUserMappingId,
      email: suggestion.email,
      outrankedBy: winner
        ? {
            speakerLabel: `Speaker ${winner.speakerIndex + 1}`,
            confidence: winner.confidence,
          }
        : null,
    };
  };

  const toggleVoiceSample = async (speakerIndex: number | null): Promise<void> => {
    if (speakerIndex == null || !meetingId.value) return;

    if (playingSpeakerIndex.value === speakerIndex) {
      stopVoiceSample();
      return;
    }
    // Tear down whatever is playing first — pausing without revoking would
    // strand its blob URL.
    stopVoiceSample();

    let blobUrl: string | null = null;
    try {
      const bytes = await api.fetchSpeakerAudio(meetingId.value, speakerIndex);
      blobUrl = URL.createObjectURL(new Blob([bytes], { type: 'audio/mpeg' }));
      voiceSampleUrl = blobUrl;
      const audio = new Audio(blobUrl);
      const url = blobUrl;
      const cleanup = () => {
        playingSpeakerIndex.value = null;
        voiceSampleAudio = null;
        if (voiceSampleUrl === url) voiceSampleUrl = null;
        URL.revokeObjectURL(url);
      };
      audio.onended = cleanup;
      audio.onerror = cleanup;
      voiceSampleAudio = audio;
      playingSpeakerIndex.value = speakerIndex;
      await audio.play();
    } catch (e) {
      // Covers the fetch failing and play() rejecting alike. A missing sample
      // is ordinary (404) and says nothing worth interrupting the host over.
      stopVoiceSample();
      if (!String(e).startsWith('404')) {
        console.error('Failed to play voice sample', e);
      }
    }
  };

  const runSearch = async (): Promise<void> => {
    const query = searchQuery.value.trim();
    const id = meetingId.value;
    if (query.length < MIN_SEARCH_LENGTH || !id) {
      searchResults.value = [];
      return;
    }
    searching.value = true;
    try {
      searchResults.value = await meetingApi.searchSpeakerMembers(id, query);
    } finally {
      searching.value = false;
    }
  };

  const debouncedSearch = (): void => {
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
      searchTimeout = null;
      void runSearch();
    }, SEARCH_DEBOUNCE_MS);
  };

  /**
   * Reset a participant row to an unassigned diarized-speaker stub: drop the
   * linked identity while keeping `participantId`, role and the rest. Used when
   * assigning a speaker steals a person from whichever row previously held them
   * — the server does the same thing, so the local list has to follow.
   */
  const detachParticipant = (
    p: MeetingParticipantInfo,
    name: string
  ): MeetingParticipantInfo => ({
    ...p,
    name,
    email: '',
    displayName: undefined,
    id: undefined,
    manualConfirm: false,
  });

  /** Forget a speaker's local label — it belongs to someone else now. */
  const releaseSpeaker = (label: string): void => {
    delete assignments.value[label];
    confirmed.value.delete(label);
  };

  /**
   * Apply a completed assignment to the local participant list.
   *
   * The server holds one person to at most one speaker index per meeting, so a
   * successful assign silently unlinks them everywhere else. Mirroring that
   * here is what stops the same person showing up twice until the next reload.
   */
  const applyAssignment = (
    speakerName: string,
    speakerIndex: number,
    label: string,
    orgUserMappingId?: number | string
  ): void => {
    for (let i = 0; i < participants.value.length; i++) {
      const p = participants.value[i];
      // Only rows that hold a diarized slot are in competition for this person.
      // A plain calendar attendee — the host's own row, typically — carries no
      // `participantId`, so the server never unlinks it and neither should we;
      // detaching it would blank a real attendee's name off the meeting.
      if (p.participantId == null || p.participantId === speakerIndex) continue;
      const takenByOrgUser = sameOrgUser(p.id, orgUserMappingId);
      const takenByLabel = orgUserMappingId == null && p.name === label;
      if (!takenByOrgUser && !takenByLabel) continue;
      const otherLabel = `Speaker ${p.participantId + 1}`;
      participants.value[i] = detachParticipant(p, otherLabel);
      releaseSpeaker(otherLabel);
    }

    // Same for labels applied optimistically this session but not yet reflected
    // in a participant row.
    for (const [otherSpeaker, otherName] of Object.entries(assignments.value)) {
      if (otherSpeaker !== speakerName && otherName === label) {
        releaseSpeaker(otherSpeaker);
      }
    }

    assignments.value[speakerName] = label;
    confirmed.value.add(speakerName);

    // The row may not exist yet: an unconfirmed diarized speaker has no
    // attendee row at all until someone says who it is.
    const idx = participants.value.findIndex((p) => p.participantId === speakerIndex);
    if (idx !== -1) {
      participants.value[idx] = {
        ...participants.value[idx],
        name: label,
        id: orgUserMappingId ?? participants.value[idx].id,
        manualConfirm: true,
      };
    } else {
      participants.value.push({
        id: orgUserMappingId,
        name: label,
        participantId: speakerIndex,
        manualConfirm: true,
      });
    }
  };

  const openAssignment = (speakerName: string): void => {
    assigningSpeaker.value = speakerName;
    searchQuery.value = '';
    searchResults.value = [];
    error.value = null;
  };

  /** Assign the open speaker to an org member found by search. */
  const assignToMember = async (member: SpeakerSearchMember): Promise<void> => {
    const speakerName = assigningSpeaker.value;
    const id = meetingId.value;
    if (!speakerName || !id) return;
    const speakerIndex = getSpeakerIndex(speakerName);
    if (speakerIndex === null) {
      error.value = `Could not resolve a speaker index for ${speakerName}`;
      closeAssignment();
      return;
    }

    try {
      await meetingApi.assignSpeaker(id, speakerIndex, {
        orgUserMappingId: member.id,
      });
      applyAssignment(
        speakerName,
        speakerIndex,
        member.name || member.email,
        member.id
      );
      closeAssignment();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to assign speaker';
    }
  };

  /** Assign the open speaker to the typed text, as an email or a plain name. */
  const assignToLabel = async (): Promise<void> => {
    const speakerName = assigningSpeaker.value;
    const id = meetingId.value;
    const label = searchQuery.value.trim();
    if (!speakerName || !id || !label) return;
    const speakerIndex = getSpeakerIndex(speakerName);
    if (speakerIndex === null) {
      error.value = `Could not resolve a speaker index for ${speakerName}`;
      closeAssignment();
      return;
    }

    const identity: SpeakerIdentity = looksLikeEmail(label)
      ? { email: label }
      : { displayName: label };
    try {
      await meetingApi.assignSpeaker(id, speakerIndex, identity);
      applyAssignment(speakerName, speakerIndex, label);
      closeAssignment();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to assign speaker';
    }
  };

  /**
   * Accept a speaker's current label — an auto-match suggestion, or a name
   * already on its participant row — and persist it as confirmed.
   */
  const confirmSpeaker = async (speakerName: string): Promise<void> => {
    const id = meetingId.value;
    if (!id) return;
    const suggestion = getSpeakerAutoMatch(speakerName);
    const displayName = getSpeakerDisplayName(speakerName);
    // Nothing to confirm: still generic, and nothing suggested.
    if (displayName === speakerName && !suggestion) return;

    let acceptedSuggestion = false;
    if (!assignments.value[speakerName] && suggestion) {
      // …unless a stronger match for the same person exists. Confirming binds
      // that person to this speaker and unlinks them from every other speaker
      // in the meeting, so accepting both suggestions in turn would hand them
      // to whichever was clicked last, regardless of score. The weaker one is
      // still shown — it may well be the same human split across two diarized
      // voices — but taking it has to be a deliberate Assign. The panel hides
      // the button; this guard is what makes that true of the behaviour and
      // not just of the markup.
      if (suggestion.outrankedBy) return;
      assignments.value[speakerName] = suggestion.name;
      acceptedSuggestion = true;
    }

    const speakerIndex = getSpeakerIndex(speakerName);
    if (speakerIndex === null) return;
    const finalName = getSpeakerDisplayName(speakerName);
    const participant = findParticipantForSpeaker(speakerName);

    // Bind to the strongest identity available so the same person can't land on
    // two rows: an existing org-user link, then the matched profile's org user,
    // then its email. A bare name is the last resort — it is a label, not an
    // identity, so it can still duplicate a differently-keyed row.
    let identity: SpeakerIdentity;
    if (participant?.id != null) {
      identity = { orgUserMappingId: participant.id };
    } else if (suggestion?.orgUserMappingId) {
      identity = { orgUserMappingId: suggestion.orgUserMappingId };
    } else if (suggestion?.email) {
      identity = { email: suggestion.email };
    } else {
      identity = looksLikeEmail(finalName)
        ? { email: finalName }
        : { displayName: finalName };
    }

    try {
      await meetingApi.assignSpeaker(id, speakerIndex, identity);
      applyAssignment(
        speakerName,
        speakerIndex,
        finalName,
        participant?.id ?? suggestion?.orgUserMappingId ?? undefined
      );
    } catch (e) {
      // Roll the optimistic suggestion back: it was never persisted, and
      // leaving it on screen would report an assignment that doesn't exist.
      if (acceptedSuggestion) delete assignments.value[speakerName];
      error.value = e instanceof Error ? e.message : 'Failed to confirm speaker';
    }
  };

  return {
    open,
    error,
    assignable,
    unresolved,
    confirmed,
    assignments,
    assigningSpeaker,
    searchQuery,
    searchResults,
    searching,
    playingSpeakerIndex,
    getSpeakerDisplayName,
    getSpeakerAutoMatch,
    getSpeakerIndex,
    toggleVoiceSample,
    debouncedSearch,
    openAssignment,
    closeAssignment,
    assignToMember,
    assignToLabel,
    confirmSpeaker,
    seedFromParticipants,
    stopVoiceSample,
    reset,
  };
}

/** Good enough to route the typed text to the email rung rather than the name
 *  rung; the server does the real validation. */
function looksLikeEmail(value: string): boolean {
  return value.includes('@') && value.includes('.');
}

/** Whether two org-user ids name the same person. Compared as strings because
 *  the same id arrives numeric from one endpoint and as a string from another;
 *  `===` would silently say "different person" and leave them on two rows. */
function sameOrgUser(
  a: number | string | undefined,
  b: number | string | undefined
): boolean {
  return a != null && b != null && String(a) === String(b);
}

export type SpeakerAssignment = ReturnType<typeof useSpeakerAssignment>;
