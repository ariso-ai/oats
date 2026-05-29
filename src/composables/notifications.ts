export interface InboxMessage {
  id: number;
  source: string;
  source_id: number | null;
  message: string | null;
  created_at: string;
  updated_at: string;
  unread: boolean;
}

export const MEETING_PREP_SOURCE = 'meeting_prep';

export interface PrepNotification {
  title: string;
  body: string;
  url: string;
}

/** Find an inbox message by source and source_id (numeric comparison). */
export function findInboxMessage(
  items: InboxMessage[],
  source: string,
  sourceId: number
): InboxMessage | null {
  return (
    items.find(
      (m) => m.source === source && Number(m.source_id) === Number(sourceId)
    ) ?? null
  );
}

/** Strip common markdown to readable plain text for an OS notification body. */
export function stripMarkdown(md: string): string {
  return md
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_`~]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Truncate to `max` characters, replacing the overflow with an ellipsis. */
export function truncate(text: string, max = 120): string {
  if (text.length <= max) return text;
  return text.slice(0, max - 1).trimEnd() + '…';
}

/** Build the OS notification content for a completed meeting prep. */
export function buildPrepNotification(
  message: string | null,
  meetingPrepId: number,
  webAppBaseUrl: string
): PrepNotification {
  const body = message
    ? truncate(stripMarkdown(message))
    : 'Your meeting prep is ready.';
  return {
    title: 'Meeting prep ready',
    body,
    url: `${webAppBaseUrl}/my/meeting-prep-v2/${meetingPrepId}`,
  };
}

/** The user's private Pusher channel name. */
export function prepChannelName(orgId: number, orgUserMappingId: number): string {
  return `private-${orgId}-${orgUserMappingId}`;
}
