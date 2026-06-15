import type { MeetingDetail } from '../composables/useBackend';

// Format an ISO timestamp for the share heading, e.g. "Jun 1, 2026, 10:00 AM".
function formatHeadingDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

/**
 * Compose the markdown document shared via the native macOS share sheet for a
 * local recording: meeting title + date heading, the AI-generated note, then the
 * user's personal note. Empty sections are omitted; returns '' when there is
 * nothing to share.
 */
export function composeLocalShareText(
  detail: Pick<MeetingDetail, 'title' | 'startAt' | 'note'>,
  personalNote: string
): string {
  const aiNote = detail.note?.trim() ?? '';
  const myNote = personalNote.trim();
  if (!aiNote && !myNote) return '';

  const title = detail.title?.trim() || 'Meeting notes';
  const date = formatHeadingDate(detail.startAt);
  const sections: string[] = [date ? `# ${title}\n${date}` : `# ${title}`];
  if (aiNote) sections.push(`## AI Notes\n\n${aiNote}`);
  if (myNote) sections.push(`## My Notes\n\n${myNote}`);
  return sections.join('\n\n');
}
