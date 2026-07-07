import { describe, it, expect } from 'vitest';
import { decideStartRecording } from './decideStartRecording';

describe('decideStartRecording', () => {
  it('local backend, detail pane closed → fresh local recording', () => {
    expect(
      decideStartRecording({ usesPicker: false, detailOpen: false, shownMeeting: null })
    ).toEqual({ kind: 'local-new' });
  });

  it('local backend, detail pane open → continue the 5-min auto-append recording', () => {
    expect(
      decideStartRecording({
        usesPicker: false,
        detailOpen: true,
        shownMeeting: { numericId: 42, title: 'Standup' },
      })
    ).toEqual({ kind: 'local-continue' });
  });

  it('ariso backend, detail pane empty → picker with no default', () => {
    expect(
      decideStartRecording({ usesPicker: true, detailOpen: false, shownMeeting: null })
    ).toEqual({ kind: 'ariso-picker', defaultMeetingId: null, defaultMeetingTitle: null });
  });

  it('ariso backend, detail pane populated → picker defaulted to the shown meeting', () => {
    expect(
      decideStartRecording({
        usesPicker: true,
        detailOpen: true,
        shownMeeting: { numericId: 42, title: 'Standup' },
      })
    ).toEqual({ kind: 'ariso-picker', defaultMeetingId: 42, defaultMeetingTitle: 'Standup' });
  });

  it('ariso backend, detail pane populated but numericId is undefined → default id gracefully null', () => {
    expect(
      decideStartRecording({
        usesPicker: true,
        detailOpen: true,
        shownMeeting: { numericId: undefined, title: 'Draft' },
      })
    ).toEqual({ kind: 'ariso-picker', defaultMeetingId: null, defaultMeetingTitle: 'Draft' });
  });
});
