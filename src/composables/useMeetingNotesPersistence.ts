import { api, local } from '../tauri';
import type { MeetingListItem } from './useBackend';

export type NotesPersistenceMode = 'local' | 'remote' | 'unsupported';

// A My-note is an editable title plus markdown body. The title is '' when the
// note has none so callers never branch on null.
export interface MeetingNote {
  content: string;
  title: string;
}

// Small seam for Library note durability. Views ask this interface where notes
// live instead of branching on local-vs-remote storage details themselves.
export interface MeetingNotesPersistence {
  modeFor(meeting: MeetingListItem): NotesPersistenceMode;
  canEdit(meeting: MeetingListItem): boolean;
  load(meeting: MeetingListItem): Promise<MeetingNote>;
  save(meeting: MeetingListItem, note: MeetingNote): Promise<void>;
}

// Shape returned by the existing personal-note API. Keeping it narrow avoids
// importing broader backend meeting-note contracts into the desktop Library.
interface IndividualNoteResponse {
  content?: string | null;
  title?: string | null;
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

// Reads from the backend path used by server-backed meeting notes. This stays
// behind the adapter so enabling remote writes later is a one-file change. The
// encoded id keeps the seam valid if a future remote source uses string ids.
async function loadRemoteNote(meeting: MeetingListItem): Promise<MeetingNote> {
  const id = encodeURIComponent(meeting.id);
  const response = await api.request('GET', `/meeting-notes/${id}/individual-note`);
  if (response.status === 404) return { content: '', title: '' };
  if (response.status !== 200) {
    throw new Error(`Remote notes unavailable (${response.status})`);
  }
  const body = response.data as IndividualNoteResponse;
  return { content: body.content ?? '', title: body.title ?? '' };
}

// Writes through the existing backend personal-note endpoint, which stores the
// requester's note on the cloud meeting without creating hidden local drafts.
// The PUT carries the title alongside content; the server already returns a
// title on GET, so it owns whether the title is persisted. The encoded id keeps
// the seam valid for string ids.
async function saveRemoteNote(meeting: MeetingListItem, note: MeetingNote): Promise<void> {
  const id = encodeURIComponent(meeting.id);
  const response = await api.request('PUT', `/meeting-notes/${id}/individual-note`, {
    content: note.content,
    title: note.title,
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
        const [content, title] = await Promise.all([
          local.readRecordingNote(meeting.id),
          local.readRecordingNoteTitle(meeting.id),
        ]);
        return { content, title };
      }
      if (modeForMeeting(meeting) === 'remote') return loadRemoteNote(meeting);
      return { content: '', title: '' };
    },

    async save(meeting, note) {
      if (isLocalRecording(meeting)) {
        await Promise.all([
          local.writeRecordingNote(meeting.id, note.content),
          local.writeRecordingNoteTitle(meeting.id, note.title),
        ]);
        return;
      }
      if (modeForMeeting(meeting) === 'remote') {
        await saveRemoteNote(meeting, note);
        return;
      }
      throw new Error('Remote Library note edits are not supported yet.');
    },
  };
}
