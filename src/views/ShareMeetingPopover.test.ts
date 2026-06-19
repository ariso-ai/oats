// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const shareMeeting = vi.fn();
const listShareEmails = vi.fn();
const sendShareEmail = vi.fn();
const unshareEmail = vi.fn();

vi.mock('../tauri', () => ({
  getDesktopConfig: () => Promise.resolve({ webAppBaseUrl: 'https://app.test', pusherKey: '', pusherCluster: '' }),
}));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({
    shareMeeting: (...a: unknown[]) => shareMeeting(...a),
    listShareEmails: (...a: unknown[]) => listShareEmails(...a),
    sendShareEmail: (...a: unknown[]) => sendShareEmail(...a),
    unshareEmail: (...a: unknown[]) => unshareEmail(...a),
  }),
}));

import ShareMeetingPopover from './ShareMeetingPopover.vue';
import type { MeetingDetail } from '../composables/useBackend';

// The component teleports all its markup into <body>, so w.find() won't see it.
// We query document.body directly and wrap elements in a tiny helper so tests
// can trigger events and read attributes with familiar syntax.
function q(selector: string): HTMLElement {
  const el = document.querySelector(selector) as HTMLElement | null;
  if (!el) throw new Error(`No element matching "${selector}" in document.body`);
  return el;
}
function qAll(selector: string): HTMLElement[] {
  return Array.from(document.querySelectorAll(selector));
}

function makeDetail(over: Partial<MeetingDetail> = {}): MeetingDetail {
  return {
    id: '5',
    title: 'Sync',
    startAt: '2026-06-01T10:00:00Z',
    isLocal: false,
    actionItems: [],
    participants: [{ id: 1, name: 'Ana', email: 'ana@x.com', role: 'host', self: true }],
    visibility: 'private',
    shareMeetingNotesToPublic: 'off',
    ...over,
  };
}

function mountPop(detail: MeetingDetail) {
  return mount(ShareMeetingPopover, {
    props: { detail, meetingId: detail.id, anchor: { bottom: 60, right: 400 } },
    attachTo: document.body,
  });
}

let wrapper: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  shareMeeting.mockReset();
  listShareEmails.mockReset();
  listShareEmails.mockResolvedValue([]);
  sendShareEmail.mockReset();
  unshareEmail.mockReset();
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

describe('ShareMeetingPopover', () => {
  it('loads shared emails on mount', async () => {
    listShareEmails.mockResolvedValue(['bob@x.com']);
    wrapper = mountPop(makeDetail());
    await flushPromises();
    expect(listShareEmails).toHaveBeenCalledWith('5');
    expect(document.body.textContent).toContain('bob'); // extra-shared tile
  });

  it('disables the public option when org gating is off', async () => {
    wrapper = mountPop(makeDetail({ shareMeetingNotesToPublic: 'off' }));
    await flushPromises();
    q('.vis-toggle').click();
    await flushPromises();
    const publicBtn = qAll('.vis-item')[0];
    expect(publicBtn.hasAttribute('disabled')).toBe(true);
  });

  it('enables public for a host when gating is host_only', async () => {
    wrapper = mountPop(makeDetail({ shareMeetingNotesToPublic: 'host_only' }));
    await flushPromises();
    q('.vis-toggle').click();
    await flushPromises();
    expect(qAll('.vis-item')[0].hasAttribute('disabled')).toBe(false);
  });

  it('shares workspace immediately and exposes a copy link', async () => {
    shareMeeting.mockResolvedValue({ shareUrl: 'https://app.test/meeting-notes/abc', shortCode: 'abc', publicShareExpiresAt: null });
    const detail = makeDetail();
    wrapper = mountPop(detail);
    await flushPromises();
    q('.vis-toggle').click();
    await flushPromises();
    qAll('.vis-item')[1].click(); // workspace
    await flushPromises();
    // The component passes a third arg (expiresInDays=undefined) for non-public shares.
    expect(shareMeeting).toHaveBeenCalledWith('5', 'workspace', undefined);
    expect(detail.shortCode).toBe('abc');
    expect(detail.visibility).toBe('workspace');
    expect(document.querySelector('.copy-link')).not.toBeNull();
  });

  it('sends a share email and shows the address as shared', async () => {
    sendShareEmail.mockResolvedValue({ alreadyShared: false });
    wrapper = mountPop(makeDetail());
    await flushPromises();
    const input = q('.email-input') as HTMLInputElement;
    input.value = 'zoe@x.com';
    input.dispatchEvent(new Event('input'));
    await flushPromises();
    q('.email-row .btn-secondary').click();
    await flushPromises();
    expect(sendShareEmail).toHaveBeenCalledWith('5', 'zoe@x.com');
    expect(document.body.textContent).toContain('zoe');
  });

  it('rejects an invalid email without calling the API', async () => {
    wrapper = mountPop(makeDetail());
    await flushPromises();
    const input = q('.email-input') as HTMLInputElement;
    input.value = 'nope';
    input.dispatchEvent(new Event('input'));
    await flushPromises();
    q('.email-row .btn-secondary').click();
    await flushPromises();
    expect(sendShareEmail).not.toHaveBeenCalled();
    const err = document.querySelector('.err');
    expect(err?.textContent).toContain('valid email');
  });

  it('unshares after the inline confirm', async () => {
    listShareEmails.mockResolvedValue(['ana@x.com']);
    unshareEmail.mockResolvedValue(undefined);
    wrapper = mountPop(makeDetail());
    await flushPromises();
    q('.avatar-lg.shared').click();
    await flushPromises();
    expect(document.querySelector('.unshare-confirm')).not.toBeNull();
    q('.btn-danger').click();
    await flushPromises();
    expect(unshareEmail).toHaveBeenCalledWith('5', 'ana@x.com');
  });

  it('public save flow calls shareMeeting with public and 30 days', async () => {
    shareMeeting.mockResolvedValue({ shareUrl: 'https://app.test/shared/meeting-notes/abc', shortCode: 'abc', publicShareExpiresAt: '2026-07-15T00:00:00Z' });
    const detail = makeDetail({
      shareMeetingNotesToPublic: 'host_only',
      participants: [{ id: 1, name: 'Ana', email: 'ana@x.com', role: 'host', self: true }],
    });
    wrapper = mountPop(detail);
    await flushPromises();
    q('.vis-toggle').click();
    await flushPromises();
    qAll('.vis-item')[0].click(); // public option
    await flushPromises();
    const saveBtn = document.querySelector('.expiry .btn-secondary') as HTMLElement;
    saveBtn.click();
    await flushPromises();
    expect(shareMeeting).toHaveBeenCalledWith('5', 'public', 30);
  });

  it('shows already-shared error when sendShareEmail returns alreadyShared', async () => {
    sendShareEmail.mockResolvedValue({ alreadyShared: true });
    wrapper = mountPop(makeDetail());
    await flushPromises();
    const input = q('.email-input') as HTMLInputElement;
    input.value = 'bob@x.com';
    input.dispatchEvent(new Event('input'));
    await flushPromises();
    q('.email-row .btn-secondary').click();
    await flushPromises();
    const err = document.querySelector('.err');
    expect(err?.textContent).toContain('Already shared');
  });
});
