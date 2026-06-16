import { describe, it, expect } from 'vitest';
import { decideRecordingAction } from './decideRecordingAction';

describe('decideRecordingAction', () => {
  it('opens the recorder with no meeting for non-picker (local) backends', () => {
    expect(
      decideRecordingAction({ view: 'today', usesPicker: false, selectedTodayId: 5, nowMeetingId: 9 })
    ).toEqual({ kind: 'record-adhoc' });
    expect(
      decideRecordingAction({ view: 'meetings', usesPicker: false, selectedTodayId: null, nowMeetingId: null })
    ).toEqual({ kind: 'record-adhoc' });
  });

  it('always opens the picker in the Meetings view, even with a selection', () => {
    expect(
      decideRecordingAction({ view: 'meetings', usesPicker: true, selectedTodayId: 42, nowMeetingId: 9 })
    ).toEqual({ kind: 'picker' });
  });

  it('records a deliberately selected today meeting in the Today view (override)', () => {
    expect(
      decideRecordingAction({ view: 'today', usesPicker: true, selectedTodayId: 42, nowMeetingId: 9 })
    ).toEqual({ kind: 'record', meetingId: 42 });
  });

  it('records the in-progress meeting in the Today view when nothing is selected', () => {
    expect(
      decideRecordingAction({ view: 'today', usesPicker: true, selectedTodayId: null, nowMeetingId: 9 })
    ).toEqual({ kind: 'record', meetingId: 9 });
  });

  it('opens the picker in the Today view with no selection and no live meeting', () => {
    expect(
      decideRecordingAction({ view: 'today', usesPicker: true, selectedTodayId: null, nowMeetingId: null })
    ).toEqual({ kind: 'picker' });
  });
});
