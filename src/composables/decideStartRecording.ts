// Decide what the Library "Start recording" button does, keyed on whether a
// meeting is currently shown in the detail pane (not on how it got there).
// Pure and side-effect free so the branching is unit-tested without mounting
// the view or stubbing Tauri.

export interface StartRecordingInput {
  /** Whether the active backend chooses meetings via the picker (Ariso). */
  usesPicker: boolean;
  /** Whether a meeting is shown in the detail pane (selectedItem != null). */
  detailOpen: boolean;
  /** The meeting shown in detail, when populated. null when empty. The picker
   *  command features it by id only (it resolves the title from its own list). */
  shownMeeting: { numericId: number | undefined } | null;
}

export type StartRecordingPlan =
  | { kind: 'local-new' }
  | { kind: 'local-continue' }
  | { kind: 'ariso-picker'; defaultMeetingId: number | null };

export function decideStartRecording(input: StartRecordingInput): StartRecordingPlan {
  if (input.usesPicker) {
    const hasDefault = input.detailOpen && input.shownMeeting !== null;
    return {
      kind: 'ariso-picker',
      defaultMeetingId: hasDefault ? input.shownMeeting!.numericId ?? null : null,
    };
  }

  return input.detailOpen ? { kind: 'local-continue' } : { kind: 'local-new' };
}
