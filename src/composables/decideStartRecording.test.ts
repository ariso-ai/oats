import { describe, it, expect } from 'vitest';
import { decideStartRecording } from './decideStartRecording';

describe('decideStartRecording', () => {
  it('returns default when no meeting is deliberately open', () => {
    expect(decideStartRecording({ usesPicker: true, openMeeting: null })).toEqual({ kind: 'default' });
    expect(decideStartRecording({ usesPicker: false, openMeeting: null })).toEqual({ kind: 'default' });
  });

  it('local backend with an open meeting → choice dialog for that recording', () => {
    expect(
      decideStartRecording({
        usesPicker: false,
        openMeeting: { id: '2026-06-02T10-00-00Z', title: 'Standup', numericId: undefined },
      })
    ).toEqual({ kind: 'local-choice', meetingTitle: 'Standup', localRecordingId: '2026-06-02T10-00-00Z' });
  });

  it('ariso backend with a numeric open meeting → picker defaulted to it', () => {
    expect(
      decideStartRecording({
        usesPicker: true,
        openMeeting: { id: '50', title: 'Sync', numericId: 50 },
      })
    ).toEqual({ kind: 'ariso-picker', defaultMeetingId: 50 });
  });

  it('ariso backend with a non-numeric open meeting → picker with no default', () => {
    expect(
      decideStartRecording({
        usesPicker: true,
        openMeeting: { id: 'draft', title: 'Draft', numericId: undefined },
      })
    ).toEqual({ kind: 'ariso-picker', defaultMeetingId: null });
  });
});
