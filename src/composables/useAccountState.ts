import { computed, ref } from 'vue';
import { SIGN_IN_CANCELED_ERROR, api, auth, type SignInResult } from '../tauri';
import { emitNotificationsSync } from './useMeetingNotifications';

export type SignInProvider = 'google' | 'microsoft';

interface Profile {
  displayName: string;
  email: string;
  avatarUrl: string;
}

const EMPTY_PROFILE: Profile = { displayName: '', email: '', avatarUrl: '' };

async function fetchUserProfile(): Promise<Profile> {
  const profile = { ...EMPTY_PROFILE };
  try {
    const res = await api.request('GET', '/auth/me');
    const data = res.data as { full_name?: string; email?: string };
    profile.displayName = data.full_name || '';
    profile.email = data.email || '';
  } catch {
    // profile fetch failed — leave fields empty
  }
  // Avatar is fetched separately and is non-critical: a failure here must not
  // disturb the name/email above, and the UI falls back to initials.
  profile.avatarUrl = await fetchAvatar();
  return profile;
}

// Which provider the user signed in with isn't recorded, so ask Google first
// and fall back to Microsoft: an account has no avatar on the other provider.
async function fetchAvatar(): Promise<string> {
  for (const path of ['/users/google-avatar', '/users/microsoft-avatar']) {
    try {
      const res = await api.request('GET', path);
      const avatar = (res.data as { avatar?: string | null } | null)?.avatar;
      // Preload before binding to the <img>: WKWebView drops the very first
      // request for a freshly-rendered <img> during the post-sign-in churn and
      // never retries it, leaving a broken "?". Loading it through a detached
      // Image first (which is not affected) warms the cache, so the bound <img>
      // resolves instantly. Falls back to initials if it truly can't load.
      if (avatar) return await preloadAvatar(avatar);
    } catch {
      // Try the next provider.
    }
  }
  return '';
}

// Resolves to `url` once it loads in a detached Image (retrying a few times for
// transient webview failures), or to '' so the caller falls back to initials.
function preloadAvatar(url: string): Promise<string> {
  return new Promise((resolve) => {
    const MAX_ATTEMPTS = 4;
    let attempts = 0;
    const attempt = () => {
      const img = new Image();
      img.referrerPolicy = 'no-referrer';
      img.onload = () => resolve(url);
      img.onerror = () => {
        attempts += 1;
        if (attempts >= MAX_ATTEMPTS) {
          resolve('');
        } else {
          setTimeout(attempt, 300 * attempts);
        }
      };
      img.src = url;
    };
    attempt();
  });
}

/**
 * The signed-in Ariso account, as one window sees it. Every Tauri window is its
 * own webview, so each calls this for its own instance and keeps it current by
 * calling `refresh()` on AUTH_CHANGED_EVENT. Callers decide when to refresh:
 * Local mode must never call it, because it hits the network.
 */
export function useAccountState() {
  const isSignedIn = ref(false);
  // False until the first refresh lands, so a view can tell "signed out" from
  // "not known yet". Later refreshes keep showing the last known state.
  const checked = ref(false);
  const displayName = ref('');
  const email = ref('');
  const avatarUrl = ref('');
  // The provider whose browser flow is pending. Both sign-in buttons disable
  // while either is pending; only the clicked one reads "Continue in your browser…".
  const signingInWith = ref<SignInProvider | null>(null);
  const errorMessage = ref('');

  const initials = computed(() => {
    const name = displayName.value || email.value || '?';
    return name.slice(0, 2).toUpperCase();
  });

  function applyProfile(profile: Profile) {
    displayName.value = profile.displayName;
    email.value = profile.email;
    avatarUrl.value = profile.avatarUrl;
  }

  // Bumped whenever this window learns the session ended, so a refresh that
  // started before then can't land its stale signed-in result afterwards.
  let epoch = 0;
  let inFlight: { epoch: number; promise: Promise<void> } | null = null;

  async function load(mine: number) {
    let signedIn = false;
    let profile = EMPTY_PROFILE;
    try {
      signedIn = !!(await auth.checkSession());
      if (signedIn) profile = await fetchUserProfile();
    } catch (e) {
      signedIn = false;
      console.warn('Failed to refresh signed-in account', e);
    }
    if (mine !== epoch) return;
    isSignedIn.value = signedIn;
    applyProfile(profile);
    checked.value = true;
  }

  /**
   * Re-read the session and, when signed in, the profile. Never throws: a
   * failure reads as signed out. Calls made while one is in flight share it,
   * so a window's own sign-in and the AUTH_CHANGED_EVENT it triggers fetch the
   * profile once.
   */
  function refresh(): Promise<void> {
    if (inFlight && inFlight.epoch === epoch) return inFlight.promise;
    const mine = epoch;
    const promise = load(mine).finally(() => {
      if (inFlight?.promise === promise) inFlight = null;
    });
    inFlight = { epoch: mine, promise };
    return promise;
  }

  /** Forget the account without asking the backend, e.g. on a switch to Local. */
  function reset() {
    epoch += 1;
    isSignedIn.value = false;
    checked.value = false;
    applyProfile(EMPTY_PROFILE);
    errorMessage.value = '';
  }

  /**
   * Run the provider's browser flow. Returns the result so the view can run
   * its own follow-ups; a cancel resolves silently with SIGN_IN_CANCELED_ERROR.
   * Emits no auth event: the backend broadcasts AUTH_CHANGED_EVENT itself.
   */
  async function signIn(provider: SignInProvider): Promise<SignInResult> {
    const startedIn = epoch;
    signingInWith.value = provider;
    errorMessage.value = '';
    try {
      const result =
        provider === 'google' ? await auth.googleSignIn() : await auth.microsoftSignIn();
      if (result.error) {
        if (result.error !== SIGN_IN_CANCELED_ERROR) {
          errorMessage.value = result.error;
        }
        return result;
      }
      // Native orchestrators (tray next-meeting, notifications) restart on the
      // bootstrap window's sync listener.
      void emitNotificationsSync().catch((err) => {
        console.warn('Failed to sync notifications after sign-in', err);
      });
      // A flow that completed after this window was reset (it switched to
      // Local) is no longer this window's to show, and must not refresh.
      if (startedIn !== epoch) return result;
      isSignedIn.value = true;
      await refresh();
      return result;
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Sign in failed';
      errorMessage.value = message;
      return { error: message };
    } finally {
      signingInWith.value = null;
    }
  }

  // Abort this window's pending browser flow. The backend resolves the waiting
  // sign-in call with the silent-cancel error, which resets the UI.
  async function cancelSignIn() {
    try {
      await auth.cancelSignIn();
    } catch {
      /* nothing pending — ignore */
    }
  }

  async function signOut() {
    await auth.signOut();
    epoch += 1;
    isSignedIn.value = false;
    applyProfile(EMPTY_PROFILE);
    void emitNotificationsSync().catch((err) => {
      console.warn('Failed to sync notifications after sign-out', err);
    });
  }

  return {
    isSignedIn,
    checked,
    displayName,
    email,
    avatarUrl,
    initials,
    signingInWith,
    errorMessage,
    refresh,
    reset,
    signIn,
    cancelSignIn,
    signOut,
  };
}
