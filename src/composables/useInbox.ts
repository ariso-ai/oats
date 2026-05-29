import { api } from '../tauri';
import type { InboxMessage } from './notifications';

/** Fetch the most recent inbox messages (newest first). Returns [] on error. */
export async function listInboxMessages(limit = 20): Promise<InboxMessage[]> {
  try {
    const res = await api.request('GET', `/user-inbox-messages?limit=${limit}`);
    if (res.status !== 200) return [];
    const data = res.data as { items?: InboxMessage[] };
    return data.items ?? [];
  } catch {
    return [];
  }
}
