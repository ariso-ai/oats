import { ref } from 'vue';
import { api } from '../tauri';

interface RawOrganizationInfo {
  organization?: { name?: unknown; logo_data?: unknown } | null;
}

// The API stores the logo inline as a base64 data URL (it validates that shape
// on write). Anything else is dropped rather than bound to an <img>, so a
// surprising value can never make the webview fetch a remote URL.
const LOGO_DATA_URL = /^data:image\/(png|jpe?g|gif|webp);base64,/i;

/**
 * The signed-in user's Ariso organization — its name and logo — for branding
 * the Library's Up Next greeting. Like `useAccountState`, callers decide when to
 * refresh: Local mode must never call it, because it hits the network.
 */
export function useOrganizationInfo() {
  const name = ref('');
  // A `data:image/...` URL, or '' when the org has no (usable) logo.
  const logo = ref('');

  // Bumped by every refresh() and reset(), so a response that lands after a
  // newer request (or after the account went away) is discarded.
  let requestId = 0;

  function apply(nextName: string, nextLogo: string) {
    name.value = nextName;
    logo.value = nextLogo;
  }

  /** Re-read the org. Never throws: a failure reads as "no org". */
  async function refresh(): Promise<void> {
    const mine = ++requestId;
    let nextName = '';
    let nextLogo = '';
    try {
      const res = await api.request('GET', '/organizations/info');
      const org = res.status === 200 ? (res.data as RawOrganizationInfo | null)?.organization : null;
      if (org) {
        if (typeof org.name === 'string') nextName = org.name.trim();
        if (typeof org.logo_data === 'string' && LOGO_DATA_URL.test(org.logo_data)) {
          nextLogo = org.logo_data;
        }
      }
    } catch (e) {
      console.warn('Failed to load organization info', e);
    }
    if (mine !== requestId) return;
    apply(nextName, nextLogo);
  }

  /** Forget the org without asking the backend, e.g. on sign-out or a switch to Local. */
  function reset() {
    requestId += 1;
    apply('', '');
  }

  return { name, logo, refresh, reset };
}
