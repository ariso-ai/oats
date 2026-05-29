import { describe, it, expect } from 'vitest';
import {
  findInboxMessage,
  stripMarkdown,
  truncate,
  buildPrepNotification,
  prepChannelName,
  type InboxMessage,
} from './notifications';

function msg(partial: Partial<InboxMessage>): InboxMessage {
  return {
    id: 1,
    source: 'meeting_prep',
    source_id: 10,
    message: 'hello',
    created_at: '2026-05-29T00:00:00Z',
    updated_at: '2026-05-29T00:00:00Z',
    unread: true,
    ...partial,
  };
}

describe('findInboxMessage', () => {
  it('finds a message by source and numeric source_id', () => {
    const items = [msg({ id: 1, source_id: 5 }), msg({ id: 2, source_id: 10 })];
    expect(findInboxMessage(items, 'meeting_prep', 10)?.id).toBe(2);
  });

  it('matches when source_id is a numeric string', () => {
    const items = [msg({ id: 3, source_id: '7' as unknown as number })];
    expect(findInboxMessage(items, 'meeting_prep', 7)?.id).toBe(3);
  });

  it('ignores other sources', () => {
    const items = [msg({ id: 4, source: 'meeting_notes', source_id: 10 })];
    expect(findInboxMessage(items, 'meeting_prep', 10)).toBeNull();
  });

  it('returns null when nothing matches', () => {
    expect(findInboxMessage([], 'meeting_prep', 10)).toBeNull();
  });
});

describe('stripMarkdown', () => {
  it('removes formatting and links, collapses whitespace', () => {
    const input = '## Prep\n\nSee [the doc](http://x.com) **now**  please';
    expect(stripMarkdown(input)).toBe('Prep See the doc now please');
  });
});

describe('truncate', () => {
  it('leaves short text unchanged', () => {
    expect(truncate('short', 120)).toBe('short');
  });

  it('truncates long text with an ellipsis', () => {
    const out = truncate('a'.repeat(200), 10);
    expect(out.length).toBe(10);
    expect(out.endsWith('…')).toBe(true);
  });
});

describe('buildPrepNotification', () => {
  it('builds title, stripped body, and deep-link url from a message', () => {
    const n = buildPrepNotification('**Prep** ready', 42, 'https://web.test');
    expect(n.title).toBe('Meeting prep ready');
    expect(n.body).toBe('Prep ready');
    expect(n.url).toBe('https://web.test/my/meeting-prep-v2/42');
  });

  it('falls back to a generic body when message is null', () => {
    const n = buildPrepNotification(null, 42, 'https://web.test');
    expect(n.body).toBe('Your meeting prep is ready.');
  });
});

describe('prepChannelName', () => {
  it('formats the private channel name', () => {
    expect(prepChannelName(3, 99)).toBe('private-3-99');
  });
});
