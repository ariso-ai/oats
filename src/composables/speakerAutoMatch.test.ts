import { describe, it, expect } from 'vitest';
import {
  AUTO_MATCH_MIN_CONFIDENCE,
  autoMatchIdentityKey,
  rankAutoMatchesByIdentity,
  speakersInTranscript,
  toAutoMatchSuggestion,
  type AutoMatchSuggestion,
} from './speakerAutoMatch';
import type { AudioSpeaker } from './useMeetingApi';

function speaker(over: Partial<AudioSpeaker> = {}): AudioSpeaker {
  return {
    speaker_index: 0,
    auto_matched_profile_id: 11,
    auto_match_confidence: 0.9,
    auto_match_name: 'Ada Lovelace',
    auto_match_org_user_mapping_id: 'oum-1',
    auto_match_email: 'ada@example.com',
    ...over,
  };
}

function suggestion(over: Partial<AutoMatchSuggestion> = {}): AutoMatchSuggestion {
  return {
    speakerIndex: 0,
    confidence: 0.9,
    orgUserMappingId: 'oum-1',
    email: 'ada@example.com',
    name: 'Ada Lovelace',
    ...over,
  };
}

describe('toAutoMatchSuggestion', () => {
  it('reduces a matched speaker to its offerable suggestion', () => {
    expect(toAutoMatchSuggestion(speaker({ speaker_index: 2 }))).toEqual({
      speakerIndex: 2,
      confidence: 0.9,
      orgUserMappingId: 'oum-1',
      email: 'ada@example.com',
      name: 'Ada Lovelace',
    });
  });

  it('drops a match below the confidence floor', () => {
    const weak = speaker({ auto_match_confidence: AUTO_MATCH_MIN_CONFIDENCE - 0.01 });
    expect(toAutoMatchSuggestion(weak)).toBeNull();
  });

  it('keeps a match sitting exactly on the floor', () => {
    const edge = speaker({ auto_match_confidence: AUTO_MATCH_MIN_CONFIDENCE });
    expect(toAutoMatchSuggestion(edge)).not.toBeNull();
  });

  it('drops a speaker with no matched profile, confidence, or name', () => {
    expect(toAutoMatchSuggestion(speaker({ auto_matched_profile_id: null }))).toBeNull();
    expect(toAutoMatchSuggestion(speaker({ auto_match_confidence: null }))).toBeNull();
    expect(toAutoMatchSuggestion(speaker({ auto_match_name: null }))).toBeNull();
  });

  it('keeps a suggestion whose identity is only an email', () => {
    const s = toAutoMatchSuggestion(speaker({ auto_match_org_user_mapping_id: null }));
    expect(s).toMatchObject({ orgUserMappingId: null, email: 'ada@example.com' });
  });
});

describe('autoMatchIdentityKey', () => {
  it('prefers the org user over the email and the name', () => {
    expect(autoMatchIdentityKey(suggestion())).toBe('oum:oum-1');
  });

  it('falls back to a case-insensitive email', () => {
    const key = autoMatchIdentityKey(
      suggestion({ orgUserMappingId: null, email: 'Ada@Example.com' })
    );
    expect(key).toBe('email:ada@example.com');
  });

  it('falls back to the bare name when there is no strong identity', () => {
    const key = autoMatchIdentityKey(
      suggestion({ orgUserMappingId: null, email: null })
    );
    expect(key).toBe('name:Ada Lovelace');
  });
});

describe('rankAutoMatchesByIdentity', () => {
  it('outranks the weaker of two matches for the same person', () => {
    // The #7194 case: 97% and 80% for one person. Confirming both in turn
    // would hand them to whichever was clicked last.
    const outranked = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 0, confidence: 0.97 }),
      suggestion({ speakerIndex: 2, confidence: 0.8 }),
    ]);
    expect(outranked.get(2)).toEqual({ speakerIndex: 0, confidence: 0.97 });
    expect(outranked.has(0)).toBe(false);
  });

  it('ranks by score, not by input order', () => {
    const outranked = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 0, confidence: 0.8 }),
      suggestion({ speakerIndex: 2, confidence: 0.97 }),
    ]);
    expect(outranked.get(0)).toEqual({ speakerIndex: 2, confidence: 0.97 });
    expect(outranked.has(2)).toBe(false);
  });

  it('leaves matches for different people alone', () => {
    const outranked = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 0, orgUserMappingId: 'oum-1' }),
      suggestion({ speakerIndex: 1, orgUserMappingId: 'oum-2', name: 'Grace Hopper' }),
    ]);
    expect(outranked.size).toBe(0);
  });

  it('breaks a tie toward the lower speaker index, whatever the input order', () => {
    const forward = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 1, confidence: 0.9 }),
      suggestion({ speakerIndex: 3, confidence: 0.9 }),
    ]);
    const reversed = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 3, confidence: 0.9 }),
      suggestion({ speakerIndex: 1, confidence: 0.9 }),
    ]);
    expect(forward.get(3)).toEqual({ speakerIndex: 1, confidence: 0.9 });
    expect(reversed.get(3)).toEqual({ speakerIndex: 1, confidence: 0.9 });
    expect(forward.has(1)).toBe(false);
    expect(reversed.has(1)).toBe(false);
  });

  it('collides two speakers matched to the same bare name', () => {
    const outranked = rankAutoMatchesByIdentity([
      suggestion({ speakerIndex: 0, orgUserMappingId: null, email: null, confidence: 0.9 }),
      suggestion({ speakerIndex: 1, orgUserMappingId: null, email: null, confidence: 0.8 }),
    ]);
    expect(outranked.get(1)).toEqual({ speakerIndex: 0, confidence: 0.9 });
  });
});

describe('speakersInTranscript', () => {
  it('lists unique speakers in first-seen order', () => {
    expect(
      speakersInTranscript([
        'Speaker 2: morning',
        'Speaker 1: morning',
        'Speaker 2: shall we start',
      ])
    ).toEqual(['Speaker 2', 'Speaker 1']);
  });

  it('strips a leading timestamp so it never lands in the speaker name', () => {
    expect(speakersInTranscript(['[00:01:12] Speaker 1: hello'])).toEqual(['Speaker 1']);
    expect(speakersInTranscript(['[1:12] Speaker 1: hello'])).toEqual(['Speaker 1']);
  });

  it('keeps real names, which is how a bot-captured transcript labels lines', () => {
    expect(speakersInTranscript(['Ada Lovelace: hello'])).toEqual(['Ada Lovelace']);
  });

  it('ignores a line with no speaker prefix', () => {
    expect(speakersInTranscript(['just some text', ': leading colon'])).toEqual([]);
  });
});
