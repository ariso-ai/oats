// Decide what the Library "Start recording" button does when a meeting is
// deliberately open in the detail pane. Pure and side-effect free so the
// branching is unit-tested without mounting the view or stubbing Tauri.

export type StartRecordingPlan =
  | { kind: 'default' }
  | { kind: 'ariso-picker'; defaultMeetingId: number | null }
  | { kind: 'local-choice'; meetingTitle: string; localRecordingId: string };

export interface StartRecordingInput {
  /** Whether the active backend chooses meetings via the picker (Ariso). */
  usesPicker: boolean;
  /** The meeting the user deliberately opened, else null. `numericId` is the
   *  backend meeting id for Ariso rows (undefined when the id isn't numeric). */
  openMeeting: { id: string; title: string; numericId: number | undefined } | null;
}

export function decideStartRecording(input: StartRecordingInput): StartRecordingPlan {
  // No deliberately-open meeting: keep today's start behavior (decideRecordingAction).
  if (!input.openMeeting) return { kind: 'default' };

  // Ariso: open the picker featuring the open meeting as the default choice.
  if (input.usesPicker) {
    return { kind: 'ariso-picker', defaultMeetingId: input.openMeeting.numericId ?? null };
  }

  // Local: offer Continue-this-recording vs New.
  return {
    kind: 'local-choice',
    meetingTitle: input.openMeeting.title,
    localRecordingId: input.openMeeting.id,
  };
}
