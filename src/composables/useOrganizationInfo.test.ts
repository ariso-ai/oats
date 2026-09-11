import { describe, it, expect, vi, beforeEach } from 'vitest';

const apiRequest = vi.fn(
  (_method: string, _path: string): Promise<{ status: number; data: unknown }> =>
    Promise.resolve({ status: 200, data: {} })
);

vi.mock('../tauri', () => ({
  api: {
    request: (method: string, path: string) => apiRequest(method, path),
  },
}));

import { useOrganizationInfo } from './useOrganizationInfo';

const LOGO = 'data:image/png;base64,iVBORw0KGgo=';

function respond(data: unknown, status = 200) {
  apiRequest.mockResolvedValueOnce({ status, data });
}

beforeEach(() => {
  apiRequest.mockReset();
  apiRequest.mockResolvedValue({ status: 200, data: {} });
});

describe('useOrganizationInfo', () => {
  it('fetches the org name and logo from /organizations/info', async () => {
    respond({ organization: { id: 7, name: 'Acme', logo_data: LOGO } });
    const org = useOrganizationInfo();
    await org.refresh();
    expect(apiRequest).toHaveBeenCalledWith('GET', '/organizations/info');
    expect(org.name.value).toBe('Acme');
    expect(org.logo.value).toBe(LOGO);
  });

  it('trims the name and treats a blank one as absent', async () => {
    respond({ organization: { name: '   ', logo_data: null } });
    const org = useOrganizationInfo();
    await org.refresh();
    expect(org.name.value).toBe('');
    expect(org.logo.value).toBe('');
  });

  it('only accepts inline image data URLs as the logo', async () => {
    respond({ organization: { name: 'Acme', logo_data: 'https://tracker.example/pixel.png' } });
    const org = useOrganizationInfo();
    await org.refresh();
    expect(org.name.value).toBe('Acme');
    expect(org.logo.value).toBe('');
  });

  it('clears the org on a non-200 response', async () => {
    respond({ organization: { name: 'Acme', logo_data: LOGO } });
    const org = useOrganizationInfo();
    await org.refresh();
    respond({ error: 'nope' }, 401);
    await org.refresh();
    expect(org.name.value).toBe('');
    expect(org.logo.value).toBe('');
  });

  it('never throws when the request fails', async () => {
    apiRequest.mockRejectedValueOnce(new Error('offline'));
    const org = useOrganizationInfo();
    await expect(org.refresh()).resolves.toBeUndefined();
    expect(org.name.value).toBe('');
  });

  it('reset() clears the org and drops a response still in flight', async () => {
    let resolve!: (v: { status: number; data: unknown }) => void;
    apiRequest.mockReturnValueOnce(new Promise((r) => { resolve = r; }));
    const org = useOrganizationInfo();
    const pending = org.refresh();
    org.reset();
    resolve({ status: 200, data: { organization: { name: 'Stale', logo_data: LOGO } } });
    await pending;
    expect(org.name.value).toBe('');
    expect(org.logo.value).toBe('');
  });

  it('lets the newest refresh win over a slower earlier one', async () => {
    let resolveFirst!: (v: { status: number; data: unknown }) => void;
    apiRequest.mockReturnValueOnce(new Promise((r) => { resolveFirst = r; }));
    respond({ organization: { name: 'New Org', logo_data: null } });
    const org = useOrganizationInfo();
    const first = org.refresh();
    await org.refresh();
    resolveFirst({ status: 200, data: { organization: { name: 'Old Org', logo_data: null } } });
    await first;
    expect(org.name.value).toBe('New Org');
  });
});
