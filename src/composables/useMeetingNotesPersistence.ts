import { api, local } from '../tauri';
import type { MeetingListItem } from './useBackend';

export type NotesPersistenceMode = 'local' | 'remote' | 'unsupported';

// Small seam for Library note durability. Views ask this interface where notes
// live instead of branching on local-vs-remote storage details themselves.
export interface MeetingNotesPersistence {
  modeFor(meeting: MeetingListItem): NotesPersistenceMode;
  canEdit(meeting: MeetingListItem): boolean;
  load(meeting: MeetingListItem): Promise<string>;
  save(meeting: MeetingListItem, markdown: string): Promise<void>;
}

// Shape returned by the existing personal-note API. Keeping it narrow avoids
// importing broader backend meeting-note contracts into the desktop Library.
interface IndividualNoteResponse {
  content?: string | null;
}

// Local meetings are identified by recording files because they have a real
// filesystem directory where `user-note.md` can be the durable artifact.
function isLocalRecording(meeting: MeetingListItem): boolean {
  return Boolean(meeting.files);
}

// Centralizes the persistence routing policy so Library views never need to
// know whether a selected meeting is backed by local files or Agents APIs.
function modeForMeeting(meeting: MeetingListItem): NotesPersistenceMode {
  if (isLocalRecording(meeting)) return 'local';
  return 'remote';
}

// Reads from the backend path used by server-backed personal notes. The
// encoded id keeps this seam valid if a future remote source uses string ids.
async function loadRemoteNote(meeting: MeetingListItem): Promise<string> {
  const id = encodeURIComponent(meeting.id);
  const response = await api.request('GET', `/meeting-notes/${id}/individual-note`);
  if (response.status === 404) return '';
  if (response.status !== 200) {
    throw new Error(`Remote notes unavailable (${response.status})`);
  }
  const body = response.data as IndividualNoteResponse;
  return body.content ?? '';
}

// Writes through Agents' personal-note endpoint, which stores the requester’s
// note on the cloud meeting without creating hidden local drafts.
async function saveRemoteNote(meeting: MeetingListItem, markdown: string): Promise<void> {
  const id = encodeURIComponent(meeting.id);
  const response = await api.request('PUT', `/meeting-notes/${id}/individual-note`, {
    content: markdown,
  });
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`Remote note save failed (${response.status})`);
  }
}

// Creates the replaceable note persistence seam used by LibraryView. It is
// intentionally a plain object rather than a provider or app-wide state system.
export function useMeetingNotesPersistence(): MeetingNotesPersistence {
  return {
    // Local recordings have a durable `user-note.md`; cloud meetings use the
    // Agents personal-note API and never fall back to localStorage drafts.
    modeFor(meeting) {
      return modeForMeeting(meeting);
    },

    canEdit(meeting) {
      return modeForMeeting(meeting) !== 'unsupported';
    },

    async load(meeting) {
      if (isLocalRecording(meeting)) {
        return local.readRecordingNote(meeting.id);
      }
      if (modeForMeeting(meeting) === 'remote') return loadRemoteNote(meeting);
      return '';
    },

    async save(meeting, markdown) {
      if (isLocalRecording(meeting)) {
        await local.writeRecordingNote(meeting.id, markdown);
        return;
      }
      if (modeForMeeting(meeting) === 'remote') {
        await saveRemoteNote(meeting, markdown);
        return;
      }
      throw new Error('Remote Library note edits are not supported yet.');
    },
  };
}
