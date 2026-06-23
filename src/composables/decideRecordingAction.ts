// Decide what the Library "start recording" button does, given the active nav
// view and the meetings in hand. Pure and side-effect free so the branching is
// unit-tested without mounting the view or stubbing Tauri.

export type RecordingAction =
  | { kind: 'record'; meetingId: number }
  | { kind: 'record-adhoc' }
  | { kind: 'picker' };

export interface RecordingDecisionInput {
  /** Active Library nav view. */
  view: 'today' | 'meetings';
  /** Whether the active backend chooses meetings via the picker (Ariso). */
  usesPicker: boolean;
  /** Numeric id of a deliberately selected *today* meeting, else null. */
  selectedTodayId: number | null;
  /** Numeric id of the meeting currently in progress ("Now"), else null. */
  nowMeetingId: number | null;
}

export function decideRecordingAction(input: RecordingDecisionInput): RecordingAction {
  // Backends without a picker (local) just open the recorder with no meeting.
  if (!input.usesPicker) return { kind: 'record-adhoc' };

  // Meetings view always defers to the picker, regardless of any selection.
  if (input.view !== 'today') return { kind: 'picker' };

  // Today: a deliberately selected today meeting wins, then the in-progress
  // meeting; otherwise hand off to the picker.
  if (input.selectedTodayId != null) return { kind: 'record', meetingId: input.selectedTodayId };
  if (input.nowMeetingId != null) return { kind: 'record', meetingId: input.nowMeetingId };
  return { kind: 'picker' };
}
