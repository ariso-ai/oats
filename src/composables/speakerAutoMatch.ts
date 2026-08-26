import type { AudioSpeaker } from './useMeetingApi';

/**
 * Below this, a match is a weaker signal than the host simply reading the
 * transcript, so it is neither shown nor offered. Ranking runs over what
 * passes this floor, so a suggestion nobody can see never suppresses one
 * they can.
 */
export const AUTO_MATCH_MIN_CONFIDENCE = 0.75;

/** A voice auto-match strong enough to offer, reduced to what ranking needs. */
export interface AutoMatchSuggestion {
  /** Diarization index (0-based) — the `Speaker N` → N-1 convention. */
  speakerIndex: number;
  confidence: number;
  orgUserMappingId: string | null;
  email: string | null;
  name: string;
}

/** The suggestion that beat an outranked one, for explaining why. */
export interface AutoMatchWinner {
  speakerIndex: number;
  confidence: number;
}

/** The offerable suggestion on a diarized speaker, or null when there is none. */
export function toAutoMatchSuggestion(
  speaker: AudioSpeaker
): AutoMatchSuggestion | null {
  if (
    !speaker.auto_matched_profile_id ||
    !speaker.auto_match_confidence ||
    speaker.auto_match_confidence < AUTO_MATCH_MIN_CONFIDENCE ||
    !speaker.auto_match_name
  ) {
    return null;
  }
  return {
    speakerIndex: speaker.speaker_index,
    confidence: speaker.auto_match_confidence,
    orgUserMappingId: speaker.auto_match_org_user_mapping_id,
    email: speaker.auto_match_email,
    name: speaker.auto_match_name,
  };
}

/**
 * The person a suggestion stands for.
 *
 * Deliberately the same ladder the assign payload is built from — org user,
 * then email, then bare name — because the question being asked is "would
 * confirming these two speakers land on one person?", and the server decides
 * that from whichever of those three the request carried. A display name is
 * the weakest rung: two different people can share one, and the server matches
 * on it all the same, so two speakers named the same way do collide there.
 */
export function autoMatchIdentityKey(suggestion: AutoMatchSuggestion): string {
  if (suggestion.orgUserMappingId) return `oum:${suggestion.orgUserMappingId}`;
  if (suggestion.email) return `email:${suggestion.email.toLowerCase()}`;
  return `name:${suggestion.name}`;
}

/**
 * Find the suggestions a stronger one already speaks for.
 *
 * One person holds at most one speaker index per meeting: confirming a speaker
 * unlinks that person from every other speaker in the meeting. So when two
 * diarized voices both match the same person — routine, since a person carries
 * several voice profiles and diarization readily splits one speaker in two —
 * accepting both in turn hands them to whichever was confirmed *last*, whatever
 * the scores were. Ranking them here is what lets the caller offer the one-click
 * accept on the stronger match only, so the outcome stops depending on click
 * order.
 *
 * Ties fall to the lower speaker index rather than to input order, so the same
 * set of suggestions always ranks the same way.
 *
 * @returns only the outranked speakers, keyed by speaker index. A speaker
 * absent from the map is the best evidence available for whoever it matched.
 */
export function rankAutoMatchesByIdentity(
  suggestions: AutoMatchSuggestion[]
): Map<number, AutoMatchWinner> {
  const bestByIdentity = new Map<string, AutoMatchSuggestion>();

  for (const suggestion of suggestions) {
    const identity = autoMatchIdentityKey(suggestion);
    const incumbent = bestByIdentity.get(identity);
    const beatsIncumbent =
      !incumbent ||
      suggestion.confidence > incumbent.confidence ||
      (suggestion.confidence === incumbent.confidence &&
        suggestion.speakerIndex < incumbent.speakerIndex);
    if (beatsIncumbent) bestByIdentity.set(identity, suggestion);
  }

  const outranked = new Map<number, AutoMatchWinner>();
  for (const suggestion of suggestions) {
    const winner = bestByIdentity.get(autoMatchIdentityKey(suggestion));
    if (!winner || winner.speakerIndex === suggestion.speakerIndex) continue;
    outranked.set(suggestion.speakerIndex, {
      speakerIndex: winner.speakerIndex,
      confidence: winner.confidence,
    });
  }
  return outranked;
}

/**
 * Speaker names appearing in transcript lines, in first-seen order.
 *
 * A stored chunk's `content` is `"Speaker 1: …"`, but tolerate a leading
 * `[HH:MM:SS] ` timestamp too — the same transcript text is rendered with one
 * elsewhere, and a line that carried it would otherwise yield a speaker named
 * `"[00:01:12] Speaker 1"`.
 */
export function speakersInTranscript(lines: string[]): string[] {
  const seen = new Set<string>();
  const found: string[] = [];
  for (const line of lines) {
    const stripped = line.replace(/^\[\d{1,2}:\d{2}(?::\d{2})?\]\s*/, '');
    const colonIdx = stripped.indexOf(': ');
    if (colonIdx > 0) {
      const name = stripped.slice(0, colonIdx);
      if (!seen.has(name)) {
        seen.add(name);
        found.push(name);
      }
    }
  }
  return found;
}
