<template>
  <div class="settings">
    <h1 class="title">Settings</h1>

    <div v-if="signInPrompt && !isSignedIn" class="signin-banner">
      Please sign in to start recording.
    </div>

    <!-- Transcription Backend Section -->
    <section class="section">
      <h2 class="section-title">Transcription Backend</h2>
      <div class="card">
        <div class="setting-row">
          <span class="setting-label">Backend</span>
          <select :value="backend" class="setting-select" @change="onSelectBackend">
            <option value="ariso">Ariso (cloud)</option>
            <option value="local">Local (on-device)</option>
          </select>
        </div>
      </div>
    </section>

    <!-- On-device models card -->
    <section v-if="backend === 'local'" class="section">
      <h2 class="section-title">On-device models</h2>
      <div class="card">
        <div v-if="modelPrompt && !sttInstalled" class="signin-banner">
          Download the models to record on your device.
        </div>
        <div class="setting-row">
          <span class="setting-label">Speech voice model</span>
          <div class="model-controls">
            <span v-if="sttInstalled" class="model-ready" title="Installed" aria-label="Installed">✓</span>
            <span v-else class="model-status">{{ sttStatusText }}</span>
            <button
              class="secondary-btn"
              :disabled="unsupported || sttInstalled || anyDownloading"
              @click="onInstallStt"
            >
              {{ sttInstalled ? 'Installed' : sttBusy === 'downloading' ? 'Downloading' : 'Install' }}
            </button>
          </div>
        </div>
        <div class="setting-row" style="margin-top: 16px">
          <span class="setting-label">Language model</span>
          <div class="model-controls">
            <span v-if="llmInstalled" class="model-ready" title="Installed" aria-label="Installed">✓</span>
            <span v-else class="model-status">{{ llmStatusText }}</span>
            <button
              class="secondary-btn"
              :disabled="unsupported || llmInstalled || anyDownloading"
              @click="onInstallLlm"
            >
              {{ llmInstalled ? 'Installed' : llmBusy === 'downloading' ? 'Downloading' : 'Install' }}
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- Account Section -->
    <section v-if="backend === 'ariso'" class="section">
      <h2 class="section-title">Account</h2>
      <div class="card">
        <div v-if="isSignedIn" class="account-info">
          <div class="avatar">{{ initials }}</div>
          <div class="account-details">
            <span class="account-name">{{ displayName }}</span>
            <span class="account-email">{{ email }}</span>
          </div>
          <button class="sign-out-btn" @click="handleSignOut">Sign Out</button>
        </div>
        <div v-else class="sign-in-container">
          <button
            :disabled="isSigningIn"
            class="google-btn"
            @click="handleGoogleSignIn"
          >
            <svg class="google-icon" viewBox="0 0 24 24">
              <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
              <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
              <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
              <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
            </svg>
            <span v-if="!isSigningIn">Sign in with Google</span>
            <span v-else>Signing in...</span>
          </button>
          <p v-if="errorMessage" class="error">{{ errorMessage }}</p>
        </div>
      </div>
    </section>

    <!-- Recording Section -->
    <section class="section">
      <h2 class="section-title">Recording</h2>
      <div class="card">
        <div class="setting-row">
          <span class="setting-label">Microphone</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              :checked="micEnabled"
              :disabled="recordingToggleBusy"
              @change="onToggleMic"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p v-if="micStatus === 'granted'" class="notif-status notif-status--ok">
          Permission granted
        </p>
        <p v-else-if="micStatus === 'denied'" class="notif-status notif-status--err">
          Permission not granted
        </p>

        <div class="setting-row" style="margin-top: 16px">
          <span class="setting-label">System Audio</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              :checked="systemAudioEnabled"
              :disabled="recordingToggleBusy"
              @change="onToggleSystemAudio"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p v-if="systemAudioStatus === 'granted'" class="notif-status notif-status--ok">
          Permission granted
        </p>
        <p v-else-if="systemAudioStatus === 'denied'" class="notif-status notif-status--err">
          Permission not granted
        </p>
      </div>
    </section>

    <!-- Notifications Section -->
    <section class="section">
      <h2 class="section-title">Notifications</h2>
      <div class="card">
        <div class="setting-row">
          <span class="setting-label">Meeting preps</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              :checked="meetingNotifications"
              @change="onToggleMeetingNotifications"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p
          v-if="notifStatus === 'granted'"
          class="notif-status notif-status--ok"
        >
          Permission granted
        </p>
        <p
          v-else-if="notifStatus === 'denied'"
          class="notif-status notif-status--err"
        >
          Permission not granted
        </p>
      </div>
    </section>

    <!-- About / Updates Section -->
    <section class="section">
      <h2 class="section-title">About</h2>
      <div class="card">
        <div class="about-header">
          <span class="version-text">Ariso {{ appVersion }}</span>
          <span class="status-line" :class="statusClass">
            {{ statusText }}
          </span>
        </div>

        <div class="update-controls">
          <button
            v-if="updateAvailable || updateSkipped"
            class="primary-btn"
            @click="showUpdateDetails"
          >Show Details</button>
          <button
            v-else
            class="secondary-btn"
            :disabled="checking"
            @click="checkNow"
          >{{ checking ? 'Checking…' : 'Check for Updates' }}</button>
        </div>

        <label class="auto-check-row">
          <input
            type="checkbox"
            :checked="autoCheck"
            @change="onToggleAutoCheck"
          />
          <span>Automatically check for updates</span>
        </label>

        <div v-if="updateError" class="error">{{ updateError }}</div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { auth, api, updater, getBackendSetting, setBackendSetting, local, type ModelStatus } from '../tauri';
import { shouldAutoDownload, rowStatusText, type Busy } from './settingsDownload';
import { applyToggle, type PermissionStatus } from './recordingSettings';
import {
  loadRecordingEnabled,
  setMicEnabled,
  setSystemAudioEnabled,
  ensureMicPermission,
  ensureSystemAudioPermission,
  checkSystemAudioPermission,
  openMicSettings,
  openSystemAudioSettings,
} from '../composables/useRecordingPermissions';
import {
  isMeetingNotificationsEnabled,
  setMeetingNotificationsEnabled,
  ensureNotificationPermission,
  openNotificationSettings,
  emitNotificationsSync,
} from '../composables/useMeetingNotifications';

const isSignedIn = ref(false);
const isSigningIn = ref(false);
const errorMessage = ref('');
const displayName = ref('');
const email = ref('');
const micEnabled = ref(true);
const systemAudioEnabled = ref(true);
const micStatus = ref<PermissionStatus>('');
const systemAudioStatus = ref<PermissionStatus>('');
const micToggling = ref(false);
const systemAudioToggling = ref(false);
// Shared across both recording toggles so one pending permission flow blocks
// the other — otherwise the user could start overlapping OS prompts.
const recordingToggleBusy = computed(
  () => micToggling.value || systemAudioToggling.value,
);
const meetingNotifications = ref(true);
const notifStatus = ref<'' | 'granted' | 'denied'>('');
const signInPrompt = ref(false);
const appVersion = __APP_VERSION__;

const backend = ref<'ariso' | 'local'>('ariso');
const modelStatus = ref<ModelStatus>({ state: 'not_downloaded' });
const modelPrompt = ref(false);

// Per-model download UI state — the STT and LLM Install buttons are independent.
const sttBusy = ref<Busy>('idle');
const llmBusy = ref<Busy>('idle');
const sttProgress = ref<number | null>(null);
const llmProgress = ref<number | null>(null);

async function refreshModelStatus() {
  try {
    modelStatus.value = await local.modelStatus();
  } catch {
    modelStatus.value = { state: 'not_downloaded' };
  }
}

async function onSelectBackend(e: Event) {
  const next = (e.target as HTMLSelectElement).value === 'local' ? 'local' : 'ariso';
  backend.value = next;
  await setBackendSetting(next);
  if (next === 'local') {
    await refreshModelStatus();
    // Auto-start the STT download (needed to record). The LLM is opt-in via its button.
    if (shouldAutoDownload(next, modelStatus.value.state)) {
      void onInstallStt();
    }
  }
}

async function onInstallStt() {
  sttBusy.value = 'downloading';
  sttProgress.value = null;
  try {
    await local.downloadStt();
    await refreshModelStatus();
    sttBusy.value = 'idle';
  } catch (e) {
    console.error('STT model download failed', e);
    sttBusy.value = 'error';
  }
}

async function onInstallLlm() {
  llmBusy.value = 'downloading';
  llmProgress.value = null;
  try {
    await local.downloadLlm();
    await refreshModelStatus();
    llmBusy.value = 'idle';
  } catch (e) {
    console.error('LLM model download failed', e);
    llmBusy.value = 'error';
  }
}

const unsupported = computed(() => modelStatus.value.state === 'unsupported');
const sttInstalled = computed(() => modelStatus.value.state === 'ready');
const llmInstalled = computed(() => modelStatus.value.llmReady === true);
const anyDownloading = computed(
  () => sttBusy.value === 'downloading' || llmBusy.value === 'downloading',
);

const sttStatusText = computed(() =>
  unsupported.value ? 'Unsupported on this device' : rowStatusText(sttBusy.value, sttProgress.value),
);
const llmStatusText = computed(() =>
  unsupported.value ? 'Unsupported on this device' : rowStatusText(llmBusy.value, llmProgress.value),
);

const checking = ref(false);
const autoCheck = ref(true);
const updateAvailable = ref(false);
const updateAvailableVersion = ref('');
const updateError = ref('');
const lastCheckUnix = ref<number | null>(null);
const skippedVersion = ref<string | null>(null);

const updateSkipped = computed(() =>
  !updateAvailable.value &&
  skippedVersion.value != null &&
  skippedVersion.value === updateAvailableVersion.value
);

const statusText = computed(() => {
  if (checking.value) return 'Checking…';
  if (updateAvailable.value) return `Update available: ${updateAvailableVersion.value}`;
  if (updateSkipped.value) return `Update ${skippedVersion.value} available (skipped)`;
  if (lastCheckUnix.value == null) return "You haven't checked yet.";
  const ago = humanizeAgo(Date.now() / 1000 - lastCheckUnix.value);
  return `You're up to date. Last checked: ${ago}`;
});

const statusClass = computed(() => {
  if (updateAvailable.value || updateSkipped.value) return 'status-available';
  if (checking.value) return 'status-checking';
  return 'status-ok';
});

function humanizeAgo(secs: number): string {
  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

async function loadUpdateState() {
  const snap = await updater.getState();
  autoCheck.value = snap.auto_check_enabled;
  lastCheckUnix.value = snap.last_check_unix;
  skippedVersion.value = snap.skipped_version;
  if (snap.latest_known) {
    updateAvailableVersion.value = snap.latest_known.version;
    updateAvailable.value = snap.latest_known.version !== snap.skipped_version;
  } else {
    updateAvailable.value = false;
    updateAvailableVersion.value = '';
  }
}

async function checkNow() {
  checking.value = true;
  updateError.value = '';
  try {
    await updater.check(true);
  } catch (e) {
    updateError.value = e instanceof Error ? e.message : String(e);
  } finally {
    checking.value = false;
    await loadUpdateState();
  }
}

async function showUpdateDetails() {
  // Re-run check with force=true; the Rust side opens (or focuses) the
  // update window. This is simpler than calling a separate "open window"
  // command and ensures the data shown is current.
  updateError.value = '';
  try {
    await updater.check(true);
  } catch (e) {
    updateError.value = e instanceof Error ? e.message : String(e);
  }
}

async function onToggleAutoCheck(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = autoCheck.value;
  autoCheck.value = checked;
  try {
    await updater.setAutoCheck(checked);
  } catch (err) {
    autoCheck.value = previous;
    updateError.value = err instanceof Error ? err.message : String(err);
  }
}

async function onToggleMeetingNotifications(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = meetingNotifications.value;
  meetingNotifications.value = checked;
  // When switching notifications on, request OS permission. If it isn't
  // granted (denied, or macOS already recorded a decision so no prompt
  // appears), open System Settings → Notifications so the user can enable
  // it manually.
  if (checked) {
    // Permission prompt + settings deep-link are best-effort: a rejection here
    // must not abort the handler, or the optimistic toggle would stay on screen
    // with nothing persisted below.
    try {
      const granted = await ensureNotificationPermission();
      notifStatus.value = granted ? 'granted' : 'denied';
      if (!granted) {
        // Previously denied / no prompt possible — let the user enable it.
        await openNotificationSettings();
      }
    } catch (err) {
      notifStatus.value = 'denied';
      console.warn('Notification permission flow failed', err);
    }
  } else {
    notifStatus.value = '';
  }
  // Revert the optimistic toggle if persisting the setting fails.
  try {
    await setMeetingNotificationsEnabled(checked);
  } catch {
    meetingNotifications.value = previous;
  }
}

const initials = computed(() => {
  const name = displayName.value || email.value || '?';
  return name.slice(0, 2).toUpperCase();
});

async function onToggleMic(e: Event) {
  if (recordingToggleBusy.value) return;
  micToggling.value = true;
  try {
    const checked = (e.target as HTMLInputElement).checked;
    const previous = micEnabled.value;
    micEnabled.value = checked;
    const res = await applyToggle(checked, previous, {
      ensurePermission: ensureMicPermission,
      openSettings: openMicSettings,
      persist: setMicEnabled,
    });
    micEnabled.value = res.enabled;
    micStatus.value = res.status;
  } finally {
    micToggling.value = false;
  }
}

async function onToggleSystemAudio(e: Event) {
  if (recordingToggleBusy.value) return;
  systemAudioToggling.value = true;
  try {
    const checked = (e.target as HTMLInputElement).checked;
    const previous = systemAudioEnabled.value;
    systemAudioEnabled.value = checked;
    const res = await applyToggle(checked, previous, {
      ensurePermission: ensureSystemAudioPermission,
      openSettings: openSystemAudioSettings,
      persist: setSystemAudioEnabled,
    });
    systemAudioEnabled.value = res.enabled;
    systemAudioStatus.value = res.status;
  } finally {
    systemAudioToggling.value = false;
  }
}

async function fetchUserProfile() {
  try {
    const res = await api.request('GET', '/auth/me');
    const data = res.data as { full_name?: string; email?: string };
    displayName.value = data.full_name || '';
    email.value = data.email || '';
  } catch {
    // profile fetch failed — leave fields empty
  }
}

let unlistenSignInPrompt: UnlistenFn | null = null;
let unlistenUpdates: UnlistenFn[] = [];

onMounted(async () => {
  const session = await auth.checkSession();
  isSignedIn.value = !!session;
  if (isSignedIn.value) {
    await fetchUserProfile();
  }

  // Bootstrap recording toggles in its own try/catch so a settings-store or
  // permission-preflight failure doesn't abort the rest of onMounted (update
  // listeners, sign-in prompt listener, backend/model state).
  try {
    const enabled = await loadRecordingEnabled();
    micEnabled.value = enabled.mic;
    systemAudioEnabled.value = enabled.systemAudio;
    // Reflect the current Screen Recording status without prompting. (Mic status
    // is intentionally left blank on load — there's no silent mic preflight as
    // clean as CGPreflightScreenCaptureAccess, and getUserMedia would prompt.)
    if (enabled.systemAudio) {
      systemAudioStatus.value = (await checkSystemAudioPermission()) ? 'granted' : 'denied';
    }
  } catch (e) {
    console.warn('Failed to initialize recording settings', e);
  }

  meetingNotifications.value = await isMeetingNotificationsEnabled();

  unlistenSignInPrompt = await listen('tray://show-sign-in-prompt', () => {
    signInPrompt.value = true;
  });

  await loadUpdateState();

  const unAvail = await listen('update://available', async () => {
    await loadUpdateState();
  });
  const unNone = await listen('update://none', async () => {
    await loadUpdateState();
    checking.value = false;
  });
  const unChecking = await listen('update://checking', () => {
    checking.value = true;
  });
  const unError = await listen<{ message: string }>('update://error', (e) => {
    updateError.value = e.payload.message;
    checking.value = false;
  });

  // Save unlisteners so onUnmounted can clear them.
  unlistenUpdates = [unAvail, unNone, unChecking, unError];

  try {
    backend.value = await getBackendSetting();
  } catch (e) {
    console.error('Failed to read backend setting; defaulting to Ariso', e);
  }
  if (backend.value === 'local') await refreshModelStatus();

  // Per-model download progress. Completion/failure is handled by the awaited
  // install calls (onInstallStt / onInstallLlm); these events only feed the bar.
  const unSttProgress = await listen<number>('model://stt/progress', (e) => {
    sttProgress.value = e.payload >= 0 ? e.payload : null;
  });
  const unLlmProgress = await listen<number>('model://llm/progress', (e) => {
    llmProgress.value = e.payload >= 0 ? e.payload : null;
  });
  const unModelPrompt = await listen('tray://show-model-prompt', () => {
    modelPrompt.value = true;
  });
  unlistenUpdates.push(unSttProgress, unLlmProgress, unModelPrompt);
});

onUnmounted(() => {
  unlistenSignInPrompt?.();
  unlistenUpdates.forEach((un) => un());
});

async function handleGoogleSignIn() {
  isSigningIn.value = true;
  errorMessage.value = '';
  try {
    const result = await auth.googleSignIn();
    if (result.error) {
      if (result.error !== 'Auth window closed') {
        errorMessage.value = result.error;
      }
      return;
    }
    isSignedIn.value = true;
    signInPrompt.value = false;
    await fetchUserProfile();
    void emitNotificationsSync().catch((err) => {
      console.warn('Failed to sync notifications after sign-in', err);
    });
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Sign in failed';
  } finally {
    isSigningIn.value = false;
  }
}

async function handleSignOut() {
  await auth.signOut();
  isSignedIn.value = false;
  displayName.value = '';
  email.value = '';
  void emitNotificationsSync().catch((err) => {
    console.warn('Failed to sync notifications after sign-out', err);
  });
}
</script>

<style scoped>
.settings {
  padding: 24px;
  font-family: -apple-system, system-ui, sans-serif;
  background: #f5f5f7;
  min-height: 100vh;
}

.title {
  font-size: 20px;
  font-weight: 700;
  margin-bottom: 24px;
  color: #1d1d1f;
}

.signin-banner {
  background: #fef3c7;
  border: 1px solid #fcd34d;
  color: #92400e;
  font-size: 13px;
  font-weight: 500;
  padding: 10px 14px;
  border-radius: 10px;
  margin-bottom: 16px;
}

.notif-status {
  margin-top: 8px;
  font-size: 13px;
  font-weight: 500;
  padding: 8px 12px;
  border-radius: 8px;
}

.notif-status--ok {
  background: #dcfce7;
  border: 1px solid #86efac;
  color: #166534;
}

.notif-status--err {
  background: #fee2e2;
  border: 1px solid #fca5a5;
  color: #991b1b;
}

.section {
  margin-bottom: 20px;
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: #86868b;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
}

.card {
  background: white;
  border-radius: 10px;
  padding: 16px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
}

.account-info {
  display: flex;
  align-items: center;
  gap: 12px;
}

.avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: #6366f1;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
  color: white;
}

.account-details {
  display: flex;
  flex-direction: column;
  flex: 1;
}

.account-name {
  font-size: 14px;
  font-weight: 500;
  color: #1d1d1f;
}

.account-email {
  font-size: 12px;
  color: #86868b;
}

.sign-out-btn {
  font-size: 13px;
  color: #f87171;
  background: none;
  border: none;
  cursor: pointer;
  font-weight: 500;
}

.sign-out-btn:hover {
  text-decoration: underline;
}

.sign-in-container {
  text-align: center;
}

.google-btn {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 10px 16px;
  background: white;
  border: 1px solid #d1d5db;
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  font-size: 14px;
  font-weight: 500;
  color: #374151;
  cursor: pointer;
  transition: background 0.15s;
}

.google-btn:hover {
  background: #f9fafb;
}

.google-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.google-icon {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
}

.error {
  margin-top: 8px;
  font-size: 12px;
  color: #f87171;
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-label {
  font-size: 14px;
  color: #1d1d1f;
}

.model-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}

.model-status {
  font-size: 13px;
  color: #6b7280;
}

.model-ready {
  color: #16a34a;
  font-size: 16px;
  font-weight: 700;
  line-height: 1;
}

.setting-select {
  font-size: 13px;
  padding: 4px 8px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  background: white;
  color: #6366f1;
}

.about-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
}

.version-text {
  font-size: 14px;
  font-weight: 500;
  color: #1d1d1f;
}

.status-line {
  font-size: 12px;
}

.status-ok       { color: #16a34a; }
.status-checking { color: #86868b; }
.status-available { color: #4f46e5; font-weight: 500; }

.update-controls {
  margin-bottom: 12px;
}

.primary-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: none;
  background: linear-gradient(to bottom, #6366f1, #4f46e5);
  color: white;
  font-weight: 500;
  cursor: pointer;
}

.secondary-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: 1px solid #d1d5db;
  background: white;
  color: #1d1d1f;
  cursor: pointer;
}

.secondary-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.auto-check-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #1d1d1f;
  cursor: pointer;
}

/* iOS-style toggle switch */
.toggle {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  cursor: pointer;
}

.toggle-input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle-track {
  display: inline-flex;
  align-items: center;
  width: 40px;
  height: 24px;
  padding: 2px;
  box-sizing: border-box;
  border-radius: 12px;
  background: #d1d5db;
  transition: background 0.2s ease;
}

.toggle-thumb {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: white;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
  transition: transform 0.2s ease;
}

.toggle-input:checked + .toggle-track {
  background: #4f46e5;
}

.toggle-input:checked + .toggle-track .toggle-thumb {
  transform: translateX(16px);
}

.toggle-input:focus-visible + .toggle-track {
  outline: 2px solid #6366f1;
  outline-offset: 2px;
}
</style>
