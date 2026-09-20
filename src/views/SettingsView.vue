<template>
  <div class="settings">
    <div v-if="showDownloadConfirm" class="download-confirm" role="dialog" aria-modal="true" aria-labelledby="download-confirm-title">
      <div class="download-confirm__card">
        <h2 id="download-confirm-title" class="download-confirm__title">Download local models?</h2>
        <p class="download-confirm__body">
          Local transcription needs the speech and notes models (~750&nbsp;MB).
          They download once and run entirely on your device.
        </p>
        <div class="download-confirm__actions">
          <button class="secondary-btn download-confirm__cancel" @click="cancelDownloadModels">Cancel</button>
          <button class="primary-btn download-confirm__confirm" @click="confirmDownloadModels">Download</button>
        </div>
      </div>
    </div>

    <div
      v-if="removeTarget"
      class="download-confirm"
      data-test="remove-confirm"
      role="dialog"
      aria-modal="true"
      aria-labelledby="remove-confirm-title"
    >
      <div class="download-confirm__card">
        <h2 id="remove-confirm-title" class="download-confirm__title">
          Remove {{ removeTarget.name }}?
        </h2>
        <p class="download-confirm__body">
          Its files are deleted from this device. Recording in Local mode needs
          this model, so it has to be downloaded again before the next meeting.
        </p>
        <div class="download-confirm__actions">
          <button
            class="secondary-btn download-confirm__cancel"
            data-test="remove-confirm-cancel"
            @click="cancelRemove"
          >
            Cancel
          </button>
          <button
            class="primary-btn download-confirm__confirm"
            data-test="remove-confirm-ok"
            @click="confirmRemove"
          >
            Remove
          </button>
        </div>
      </div>
    </div>

    <h1 class="title">Settings</h1>

    <div v-if="signInPrompt && !isSignedIn" class="signin-banner">
      Please sign in to start recording.
    </div>

    <!-- Transcription Backend Section -->
    <section class="section">
      <div class="card">
        <div class="setting-row">
          <span id="backend-label" class="setting-label">Backend</span>
          <div
            ref="backendSelectRef"
            class="backend-select"
            @focusout="onBackendFocusOut"
            @keydown.escape.prevent="closeBackendMenu"
          >
            <button
              ref="backendTriggerRef"
              type="button"
              class="backend-trigger"
              :disabled="recordingActive"
              aria-haspopup="listbox"
              :aria-expanded="backendOpen"
              aria-controls="backend-listbox"
              @click="toggleBackendMenu"
              @keydown.down.prevent="openBackendMenu(0)"
              @keydown.up.prevent="openBackendMenu(backendOptions.length - 1)"
              @keydown.enter.prevent="toggleBackendMenu"
              @keydown.space.prevent="toggleBackendMenu"
            >
              <span class="backend-trigger-text">{{ currentBackend.label }}</span>
              <svg v-if="backend === 'ariso'" class="backend-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
              </svg>
              <svg v-else class="backend-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                <line x1="8" y1="21" x2="16" y2="21" />
                <line x1="12" y1="17" x2="12" y2="21" />
              </svg>
              <svg class="backend-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </button>
            <ul
              v-if="backendOpen"
              id="backend-listbox"
              class="backend-menu"
              role="listbox"
              aria-labelledby="backend-label"
            >
              <li
                v-for="(opt, idx) in backendOptions"
                :key="opt.value"
                class="backend-option"
                :class="{ 'backend-option--active': backend === opt.value }"
                role="option"
                :aria-selected="backend === opt.value"
                tabindex="-1"
                @mousedown.prevent="selectBackend(opt.value)"
                @keydown.down.prevent="focusOption(idx + 1)"
                @keydown.up.prevent="focusOption(idx - 1)"
                @keydown.home.prevent="focusOption(0)"
                @keydown.end.prevent="focusOption(backendOptions.length - 1)"
                @keydown.enter.prevent="selectBackend(opt.value)"
                @keydown.space.prevent="selectBackend(opt.value)"
              >
                <span>{{ opt.label }}</span>
                <svg v-if="opt.value === 'ariso'" class="backend-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
                </svg>
                <svg v-else class="backend-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                  <line x1="8" y1="21" x2="16" y2="21" />
                  <line x1="12" y1="17" x2="12" y2="21" />
                </svg>
              </li>
            </ul>
          </div>
        </div>
        <p v-if="recordingActive" class="setting-hint">
          Backend can't be changed while recording.
        </p>
      </div>
    </section>

    <!-- Language models -->
    <section v-if="backend === 'local'" class="section" data-test="models-section">
      <h2 class="section-title">Language models</h2>
      <div class="card">
        <div v-if="showModelBanner" class="signin-banner">
          Both local models must finish downloading before you can record.
        </div>
        <table class="model-table">
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col" class="model-type-head">Type</th>
              <th scope="col" class="model-runtime-head">Runtime</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in catalog"
              :key="row.key"
              class="model-row"
              :class="{
                'model-row--selectable': true,
                'model-row--active': isActiveModel(row),
              }"
              data-test="model-row"
              :aria-selected="isActiveModel(row)"
              @click="onRowClick(row)"
            >
              <td class="model-name">
                <span class="cell-flex">
                  {{ row.name }}
                  <span
                    v-if="isActiveModel(row)"
                    class="model-tick model-tick--on"
                    title="Currently in use"
                    aria-label="Currently in use"
                  >✓</span>
                </span>
              </td>
              <td class="model-type">
                <span class="help">
                  <span
                    class="model-type-icon"
                    data-test="model-type-icon"
                    role="img"
                    tabindex="0"
                    :aria-label="`${row.type === 'Speech' ? 'Speech model' : 'Language model'}. ${row.details}`"
                  >
                    <!-- Speech: a microphone. Language: lines of text. -->
                    <svg v-if="row.type === 'Speech'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                      <line x1="12" y1="19" x2="12" y2="23" />
                    </svg>
                    <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                      <polyline points="14 2 14 8 20 8" />
                      <line x1="8" y1="13" x2="16" y2="13" />
                      <line x1="8" y1="17" x2="13" y2="17" />
                    </svg>
                  </span>
                  <span role="tooltip" class="help-tooltip" data-test="model-details">{{ row.details }}</span>
                </span>
              </td>
              <td class="model-runtime">
                <span class="cell-flex cell-flex--end">
                <template v-if="row.runtime === 'local'">
                  <span class="model-size" data-test="model-size">{{ rowSize(row) }}</span>
                  <span v-if="rowDetail(row)" class="model-status">{{ rowDetail(row) }}</span>
                  <button
                    v-if="rowInstalled(row)"
                    class="icon-btn"
                    data-test="remove-model"
                    title="Delete"
                    aria-label="Delete"
                    :disabled="recordingActive || anyDownloading"
                    @click.stop="onRemoveRow(row)"
                  >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
                      <path d="M10 11v6M14 11v6" />
                      <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
                    </svg>
                  </button>
                  <button
                    v-else-if="!unsupported"
                    class="secondary-btn"
                    :disabled="anyDownloading"
                    @click.stop="onInstallRow(row)"
                  >
                    {{ rowBusy(row) === 'downloading' ? 'Downloading' : 'Install' }}
                  </button>
                </template>
                <span v-else>Remote</span>
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="removeError" class="setting-hint" style="color: var(--danger, #c0392b)">
          {{ removeError }}
        </p>
      </div>
    </section>

    <!-- Vault -->
    <section v-if="backend === 'local'" class="section" data-test="vault-section">
      <h2 class="section-title">Vault</h2>
      <div class="card">
        <div class="setting-row">
          <span class="label-with-help">
            <span class="setting-label">Vault location</span>
            <span class="help">
              <button
                type="button"
                class="help-btn"
                data-test="vault-help"
                aria-label="About vault location"
                aria-describedby="vault-help-text"
              >
                ?
              </button>
              <span id="vault-help-text" role="tooltip" class="help-tooltip">
                Notes and audio are saved as a local Obsidian vault here.
                Choosing a new folder starts a fresh, empty vault — existing
                recordings stay in the old folder and aren't moved. If you sync
                this folder (iCloud, Obsidian Sync, etc.), those notes and audio
                leave this device.
              </span>
            </span>
          </span>
          <div class="model-controls">
            <span class="vault-path" data-test="vault-path" :title="vaultDir">{{ vaultDirDisplay }}</span>
            <button
              class="secondary-btn"
              data-test="change-vault"
              :disabled="recordingActive"
              @click="onChangeVault"
            >
              Change…
            </button>
          </div>
        </div>
        <p v-if="vaultError" class="setting-hint" style="color: var(--danger, #c0392b)">
          {{ vaultError }}
        </p>
      </div>
    </section>

    <!-- Account Section -->
    <section v-if="backend === 'ariso'" class="section">
      <h2 class="section-title">Account</h2>
      <div class="card">
        <!-- template wrapper so the sign-in v-else stays adjacent to this
             v-if: an element between them would silently re-pair the v-else. -->
        <template v-if="isSignedIn">
        <div class="account-info">
          <img
            v-if="avatarUrl"
            class="avatar"
            :src="avatarUrl"
            :alt="displayName || email"
            referrerpolicy="no-referrer"
            @error="avatarUrl = ''"
          />
          <div v-else class="avatar">{{ initials }}</div>
          <div class="account-details">
            <span class="account-name">{{ displayName }}</span>
            <span class="account-email">{{ email }}</span>
          </div>
          <button class="sign-out-btn" @click="signOut">Sign Out</button>
        </div>
        <div v-if="calendarConnected === false" class="calendar-connect">
          <p class="calendar-connect-text">
            Calendar isn’t connected, so oats can’t see your meetings.
          </p>
          <button
            :disabled="isConnectingCalendar"
            class="secondary-btn"
            @click="refreshCalendarAccess"
          >
            {{ isConnectingCalendar ? 'Continue in your browser…' : 'Connect Calendar' }}
          </button>
        </div>
        </template>
        <div v-else class="sign-in-container">
          <SignInButtons
            :signing-in-with="signingInWith"
            :error-message="errorMessage"
            @sign-in="handleSignIn"
            @cancel="cancelSignIn"
          />
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
              :disabled="recordingToggleBusy || !systemAudioSupported"
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
        <p v-else-if="!systemAudioSupported" class="notif-status notif-status--err">
          System audio capture is not available on this platform yet.
        </p>

        <div class="setting-row" style="margin-top: 16px">
          <span class="setting-label">Auto-record meetings</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              :checked="autoRecordEnabled"
              :disabled="!autoRecordSupported || !micEnabled"
              @change="onToggleAutoRecord"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p v-if="!autoRecordSupported" class="notif-status notif-status--err">
          Auto-record is not available on this platform.
        </p>
        <!-- Meeting detection (the prompt this setting configures) only runs
             while the microphone is enabled. -->
        <p v-else-if="!micEnabled" class="setting-hint">
          Turn on Microphone to detect meetings.
        </p>

        <div class="setting-row" style="margin-top: 16px">
          <span class="setting-label">Silence detection</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              :checked="silenceDetectionEnabled"
              @change="onToggleSilenceDetection"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p class="setting-hint">
          Prompts you before auto-stopping a recording that's gone quiet.
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
        <div class="setting-row" style="margin-top: 16px">
          <span id="meeting-end-reminder-label" class="setting-label">Meeting stop reminder</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              aria-labelledby="meeting-end-reminder-label"
              :checked="meetingEndReminder"
              @change="onToggleMeetingEndReminder"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p class="setting-hint">
          Prompts you to keep or stop recording once a calendar meeting's
          scheduled end has passed.
        </p>
      </div>
    </section>

    <!-- Privacy Section -->
    <section class="section">
      <h2 class="section-title">Privacy</h2>
      <div class="card">
        <div class="setting-row">
          <span id="diagnostics-label" class="setting-label">Collect diagnostic data</span>
          <label class="toggle">
            <input
              type="checkbox"
              class="toggle-input"
              aria-labelledby="diagnostics-label"
              :checked="diagnosticsEnabled"
              @change="onToggleDiagnostics"
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
          </label>
        </div>
        <p class="setting-hint">
          Sends anonymous error reports (such as failed recording uploads) to help
          us fix bugs. Never includes audio, transcripts, or meeting notes.
        </p>
        <p v-if="diagnosticsEnabled && backend === 'local'" class="notif-status notif-status--err">
          Paused while oats is running on-device — nothing leaves your device in this mode.
        </p>
      </div>
    </section>

    <!-- About / Updates Section -->
    <section class="section">
      <h2 class="section-title">About</h2>
      <div class="card">
        <div class="about-header">
          <span class="version-text">oats {{ appVersion }}</span>
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
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { BACKEND_CHANGED_EVENT } from '../composables/useBackend';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { AUTH_CHANGED_EVENT, auth, updater, getBackendSetting, setBackendSetting, hasPromptedLocalModels, setPromptedLocalModels, getNotesModelSetting, setNotesModelSetting, getSpeechModelSetting, setSpeechModelSetting, local, getVaultDir, setVaultDir, pickVaultFolder, type ModelStatus, type ModelSizes, type LocalModelKind } from '../tauri';
import { DEFAULT_NOTES_MODEL, notesModelKey, type NotesModelId } from '../notesModels';
import { modelCatalog, formatModelSize, DEFAULT_SPEECH_MODEL_KEY, type CatalogModel } from '../modelCatalog';
import { shouldPromptDownload, rowDetailText, pendingInstalls, modelBannerVisible, type Busy } from './settingsDownload';
import { defaultPlatformCapabilities, loadPlatformCapabilities } from '../composables/usePlatformCapabilities';
import { applyToggle, type PermissionStatus } from './recordingSettings';
import { isDiagnosticsEnabled, setDiagnosticsEnabled } from '../composables/useDiagnostics';
import {
  loadRecordingEnabled,
  setMicEnabled,
  setSystemAudioEnabled,
  ensureMicPermission,
  ensureSystemAudioPermission,
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
import {
  isAutoRecordEnabled,
  setAutoRecordEnabled,
  isAutoRecordSupported,
} from '../composables/useAutoRecord';
import {
  isSilenceDetectionEnabled,
  setSilenceDetectionEnabled,
} from '../composables/useSilenceDetection';
import {
  isMeetingEndReminderEnabled,
  setMeetingEndReminderEnabled,
} from '../composables/useMeetingEndReminder';
import { useAccountState, type SignInProvider } from '../composables/useAccountState';
import SignInButtons from './SignInButtons.vue';

const {
  isSignedIn,
  displayName,
  email,
  avatarUrl,
  initials,
  signingInWith,
  // Also carries the platform-capabilities failure below, shown in the same spot.
  errorMessage,
  refresh: refreshSignedInAccount,
  signIn,
  cancelSignIn,
  signOut,
} = useAccountState();
const isSigningIn = computed(() => signingInWith.value !== null);
const isConnectingCalendar = ref(false);
// null = not checked this session; false = checked and missing.
const calendarConnected = ref<boolean | null>(null);
// The Calendar verdict belongs to the session that just ended, however it
// ended: a sign-out here, or one that reached this window as a broadcast. The
// next session may be a Microsoft sign-in finished in another window, which
// reaches this window only through the refresh, and that keeps the verdict.
watch(isSignedIn, (signedIn) => {
  if (!signedIn) calendarConnected.value = null;
});
const micEnabled = ref(true);
const systemAudioEnabled = ref(true);
const autoRecordEnabled = ref(true);
const autoRecordSupported = ref(true);
const silenceDetectionEnabled = ref(true);
const meetingEndReminder = ref(true);
const micStatus = ref<PermissionStatus>('');
const systemAudioStatus = ref<PermissionStatus>('');
const micToggling = ref(false);
const systemAudioToggling = ref(false);
// Shared across both recording toggles so one pending permission flow blocks
// the other — otherwise the user could start overlapping OS prompts.
const recordingToggleBusy = computed(
  () => micToggling.value || systemAudioToggling.value,
);
// Opt-in diagnostics (issue #260). Defaults to false so a load failure leaves
// the user opted out rather than silently opted in.
const diagnosticsEnabled = ref(false);
const meetingNotifications = ref(true);
const notifStatus = ref<'' | 'granted' | 'denied'>('');
const signInPrompt = ref(false);
const appVersion = __APP_VERSION__;
// Seed with a render-safe snapshot so the pre-created Settings window never
// blocks on IPC. `onMounted` replaces it with native truth before model and
// recording support are evaluated.
const platformCapabilities = ref(defaultPlatformCapabilities());

const backend = ref<'ariso' | 'local'>('ariso');
const modelStatus = ref<ModelStatus>({ state: 'not_downloaded' });
const modelPrompt = ref(false);
const showDownloadConfirm = ref(false);

// Per-model download UI state — the STT and LLM Install buttons are independent.
const sttBusy = ref<Busy>('idle');
const llmBusy = ref<Busy>('idle');
const sttProgress = ref<number | null>(null);
const llmProgress = ref<number | null>(null);

async function refreshModelStatus() {
  if (!platformCapabilities.value.localBackend.supported) {
    modelStatus.value = { state: 'unsupported' };
    return;
  }
  try {
    modelStatus.value = await local.modelStatus();
  } catch {
    modelStatus.value = { state: 'not_downloaded' };
  }
}

const vaultDir = ref('');
const vaultError = ref('');

// Show the tail of the path (its most specific part) under 20 chars, with a
// leading "..." when truncated. The full path stays available via the `title`
// attribute on hover.
const vaultDirDisplay = computed(() => {
  const path = vaultDir.value;
  const MAX = 20;
  if (path.length <= MAX) return path;
  return '...' + path.slice(-(MAX - 3));
});

async function loadVaultDir() {
  try {
    vaultDir.value = await getVaultDir();
  } catch (e) {
    console.error('Failed to read vault dir', e);
  }
}

async function onChangeVault() {
  if (recordingActive.value) return;
  vaultError.value = '';
  try {
    const picked = await pickVaultFolder(vaultDir.value || undefined);
    if (!picked || picked === vaultDir.value) return;
    await setVaultDir(picked);
    vaultDir.value = picked;
  } catch (e) {
    vaultError.value = e instanceof Error ? e.message : String(e);
    // `set_vault_dir` sets the in-memory override before it may fail on
    // ensure/persist, so resync the displayed path with the true active
    // vault rather than assuming the old one is still correct.
    await loadVaultDir();
  }
}

// --- Language models table (Local backend only) ----------------------------
// The table is the single control: clicking a Notes row makes that model the
// one that writes notes, and each local row carries its own install button.
const notesModel = ref<NotesModelId>(DEFAULT_NOTES_MODEL);
const catalog = modelCatalog();

async function loadNotesModel() {
  try {
    notesModel.value = await getNotesModelSetting();
  } catch (e) {
    console.error('Failed to read notes model setting', e);
  }
}

const speechModelKey = ref<string>(DEFAULT_SPEECH_MODEL_KEY);

async function loadSpeechModel() {
  try {
    speechModelKey.value = await getSpeechModelSetting();
  } catch (e) {
    console.error('Failed to read speech model setting', e);
  }
}

/** The tick marks the model of each type that is actually in use: one speech
 *  model transcribes, one notes model writes the notes. */
function isActiveModel(row: CatalogModel): boolean {
  if (row.type === 'Speech') return row.key === speechModelKey.value;
  return (
    !!row.notesModel && notesModelKey(row.notesModel) === notesModelKey(notesModel.value)
  );
}

/** Speech rows read the STT download state; every other row is a notes model. */
function rowInstalled(row: CatalogModel): boolean {
  return row.type === 'Speech' ? sttInstalled.value : llmInstalled.value;
}

function rowBusy(row: CatalogModel): Busy {
  return row.type === 'Speech' ? sttBusy.value : llmBusy.value;
}

/** Text beside the size: progress while downloading, and failures — but not
 *  "not downloaded", which the missing tick and the Install button already say. */
function rowDetail(row: CatalogModel): string {
  const progress = row.type === 'Speech' ? sttProgress.value : llmProgress.value;
  return rowDetailText(rowBusy(row), progress, rowInstalled(row), unsupported.value);
}

const modelSizes = ref<ModelSizes>({ notes: null, speech: null });
const removeTarget = ref<CatalogModel | null>(null);
const removeError = ref('');

async function loadModelSizes() {
  try {
    modelSizes.value = await local.modelSizes();
  } catch (e) {
    console.error('Failed to read model sizes', e);
    modelSizes.value = { notes: null, speech: null };
  }
}

function rowKind(row: CatalogModel): LocalModelKind {
  return row.type === 'Speech' ? 'speech' : 'notes';
}

function rowSize(row: CatalogModel): string {
  return formatModelSize(modelSizes.value[rowKind(row)]);
}

function onRemoveRow(row: CatalogModel) {
  removeError.value = '';
  removeTarget.value = row;
}

function cancelRemove() {
  removeTarget.value = null;
}

async function confirmRemove() {
  const row = removeTarget.value;
  removeTarget.value = null;
  if (!row) return;
  try {
    await local.deleteModel(rowKind(row));
  } catch (e) {
    // The backend refuses mid-recording and mid-download; say which, rather
    // than leaving the row looking installed for no stated reason.
    removeError.value = e instanceof Error ? e.message : String(e);
  }
  await refreshModelStatus();
  await loadModelSizes();
}

function onInstallRow(row: CatalogModel) {
  if (row.runtime !== 'local') return;
  if (row.type === 'Speech') void onInstallStt();
  else void onInstallLlm();
}

async function onRowClick(row: CatalogModel) {
  if (row.type === 'Speech') {
    const previousKey = speechModelKey.value;
    if (row.key === previousKey) return;
    speechModelKey.value = row.key;
    try {
      await setSpeechModelSetting(row.key);
    } catch (e) {
      console.error('Failed to persist speech model', e);
      speechModelKey.value = previousKey;
    }
    return;
  }
  if (!row.notesModel) return;
  const previous = notesModel.value;
  notesModel.value = row.notesModel;
  try {
    await setNotesModelSetting(row.notesModel);
  } catch (e) {
    console.error('Failed to persist notes model', e);
    notesModel.value = previous;
  }
}

const backendOptions = [
  { value: 'ariso', label: 'ariso.ai' },
  { value: 'local', label: 'Local' },
] as const;
const backendOpen = ref(false);
const recordingActive = ref(false);

// Recording runs in the separate "waveform" window; its presence is the
// source of truth on mount/focus, and recording://state keeps it live while
// this (persistent) window stays open in the background.
async function refreshRecordingState() {
  try {
    const wins = await getAllWebviewWindows();
    recordingActive.value = wins.some((w) => w.label === 'waveform');
  } catch (e) {
    console.error('Failed to read window state', e);
  }
}

function onWindowFocus() {
  void refreshRecordingState();
}

watch(recordingActive, (active) => {
  if (active) backendOpen.value = false;
});

const backendSelectRef = ref<HTMLElement | null>(null);
const backendTriggerRef = ref<HTMLButtonElement | null>(null);
const currentBackend = computed(
  () => backendOptions.find((o) => o.value === backend.value) ?? backendOptions[0],
);

function focusOption(idx: number) {
  const wrapper = backendSelectRef.value;
  if (!wrapper) return;
  const options = wrapper.querySelectorAll<HTMLElement>('.backend-option');
  if (options.length === 0) return;
  const wrapped = ((idx % options.length) + options.length) % options.length;
  options[wrapped]?.focus();
}

async function openBackendMenu(focusIdx: number) {
  if (!backendOpen.value) {
    backendOpen.value = true;
    await nextTick();
  }
  focusOption(focusIdx);
}

function closeBackendMenu() {
  if (!backendOpen.value) return;
  backendOpen.value = false;
  backendTriggerRef.value?.focus();
}

function toggleBackendMenu() {
  if (backendOpen.value) {
    closeBackendMenu();
  } else {
    const selectedIdx = backendOptions.findIndex((o) => o.value === backend.value);
    void openBackendMenu(selectedIdx >= 0 ? selectedIdx : 0);
  }
}

function onBackendFocusOut(e: FocusEvent) {
  // Close when focus moves outside the wrapper (e.g., Tab away or click
  // elsewhere). Keep open when focus moves between trigger and options.
  const next = e.relatedTarget as Node | null;
  if (!next || !backendSelectRef.value?.contains(next)) {
    backendOpen.value = false;
  }
}

async function selectBackend(next: 'ariso' | 'local') {
  backendOpen.value = false;
  backendTriggerRef.value?.focus();
  if (recordingActive.value) return;
  if (next === backend.value) return;
  backend.value = next;
  await setBackendSetting(next);
  // Tell other windows (the Library) the active backend changed so they drop
  // any meeting held open from the previous backend and reload.
  void emit(BACKEND_CHANGED_EVENT).catch((err) => {
    console.warn('Failed to broadcast backend change', err);
  });
  // Native orchestrators (tray next-meeting, notifications) re-evaluate
  // their backend/session gates via the bootstrap window's SYNC listener.
  void emitNotificationsSync().catch((err) => {
    console.warn('Failed to broadcast sync after backend change', err);
  });
  if (next === 'local') await afterSwitchToLocal();
}

async function afterSwitchToLocal() {
  await refreshModelStatus();
  // First time only: ask before fetching the (large) on-device models.
  const prompted = await hasPromptedLocalModels().catch(() => true);
  if (shouldPromptDownload('local', prompted, modelStatus.value.state)) {
    showDownloadConfirm.value = true;
  } else {
    // Already confirmed once before (or STT already installed): skip the
    // modal and start any still-missing downloads right away, so the models
    // are ready by the time the user records.
    startMissingDownloads();
  }
}

// Another window (the Meetings window's backend menu) switched the backend.
// This window is pre-created and outlives that, so follow it, and treat a
// switch to Local the way a switch made here is treated. Its own switches
// come back here too, already applied, so an unchanged backend is a no-op.
async function onBackendChangedElsewhere() {
  let next: 'ariso' | 'local';
  try {
    next = await getBackendSetting();
  } catch (e) {
    console.warn('Failed to re-read backend setting', e);
    return;
  }
  if (next === backend.value) return;
  backend.value = next;
  backendOpen.value = false;
  if (next === 'local') {
    await afterSwitchToLocal();
  } else {
    // Nothing left to confirm: the switch the modal was about is undone.
    showDownloadConfirm.value = false;
  }
}

async function confirmDownloadModels() {
  showDownloadConfirm.value = false;
  // Best-effort flag write; downloads proceed regardless.
  await setPromptedLocalModels(true).catch((e) =>
    console.warn('Failed to persist localModelsPrompted', e),
  );
  // Per-target Rust guards allow STT and LLM to download in parallel.
  void onInstallStt();
  void onInstallLlm();
}

async function cancelDownloadModels() {
  showDownloadConfirm.value = false;
  // Local is unusable without models — fall back to Ariso. Do NOT set the
  // prompted flag, so a later switch to Local will ask again.
  backend.value = 'ariso';
  await setBackendSetting('ariso');
  void emit(BACKEND_CHANGED_EVENT).catch((err) => {
    console.warn('Failed to broadcast backend change', err);
  });
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

// Kick off downloads for whichever on-device models are still missing. Shared
// by the backend switch and the recording-gate prompt. Reads the current
// modelStatus, so callers refresh it first. The Rust per-target guards de-dupe,
// so calling this while a download is already in progress is a safe no-op.
function startMissingDownloads() {
  const pending = pendingInstalls(modelStatus.value, sttBusy.value, llmBusy.value);
  if (pending.stt) void onInstallStt();
  if (pending.llm) void onInstallLlm();
}

const unsupported = computed(() => modelStatus.value.state === 'unsupported');
// This value controls availability copy and interaction only; OS permission is
// a separate concern handled when the user actually enables capture.
const systemAudioSupported = computed(() => platformCapabilities.value.systemAudio.supported);
const sttInstalled = computed(() => modelStatus.value.state === 'ready');
const llmInstalled = computed(() => modelStatus.value.llmReady === true);
const anyDownloading = computed(
  () => sttBusy.value === 'downloading' || llmBusy.value === 'downloading',
);

// Hide the banner on unsupported platforms (neither model can install there) so
// it doesn't linger forever; otherwise show it while either model is incomplete.
const showModelBanner = computed(() =>
  modelBannerVisible(
    modelPrompt.value,
    unsupported.value || sttInstalled.value,
    unsupported.value || llmInstalled.value,
  ),
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

async function onToggleAutoRecord(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = autoRecordEnabled.value;
  autoRecordEnabled.value = checked;
  try {
    await setAutoRecordEnabled(checked);
  } catch {
    autoRecordEnabled.value = previous;
  }
}

async function onToggleSilenceDetection(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = silenceDetectionEnabled.value;
  silenceDetectionEnabled.value = checked;
  try {
    await setSilenceDetectionEnabled(checked);
  } catch {
    silenceDetectionEnabled.value = previous;
  }
}

async function onToggleDiagnostics(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = diagnosticsEnabled.value;
  diagnosticsEnabled.value = checked;
  try {
    await setDiagnosticsEnabled(checked);
  } catch {
    diagnosticsEnabled.value = previous;
  }
}

async function onToggleMeetingEndReminder(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const previous = meetingEndReminder.value;
  meetingEndReminder.value = checked;
  try {
    await setMeetingEndReminderEnabled(checked);
  } catch {
    meetingEndReminder.value = previous;
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

let unlistenSignInPrompt: UnlistenFn | null = null;
const unlistenUpdates: UnlistenFn[] = [];

onMounted(async () => {
  // Native capabilities are authoritative. Keep the conservative initial state
  // and report an integration failure instead of guessing support from the UA.
  try {
    platformCapabilities.value = await loadPlatformCapabilities();
  } catch (error) {
    console.error('Failed to load platform capabilities', error);
    errorMessage.value = 'Platform features are unavailable. Restart oats and try again.';
  }
  await refreshSignedInAccount();

  // Bootstrap recording toggles in its own try/catch so a settings-store or
  // permission-preflight failure doesn't abort the rest of onMounted (update
  // listeners, sign-in prompt listener, backend/model state).
  try {
    const enabled = await loadRecordingEnabled();
    micEnabled.value = enabled.mic;
    systemAudioEnabled.value = enabled.systemAudio;
    autoRecordSupported.value = await isAutoRecordSupported();
    autoRecordEnabled.value = await isAutoRecordEnabled();
    silenceDetectionEnabled.value = await isSilenceDetectionEnabled();
    meetingEndReminder.value = await isMeetingEndReminderEnabled();
    // Both mic and system-audio status are left blank on load: there's no
    // silent preflight for either (getUserMedia prompts; the system-audio
    // probe creates a process tap, which trips the TCC dialog on first use).
    // The status fills in when the user toggles the row.
  } catch (e) {
    console.warn('Failed to initialize recording settings', e);
  }

  meetingNotifications.value = await isMeetingNotificationsEnabled();
  // Isolated: a settings-store read failure must leave diagnostics off, not
  // abort the remaining onMounted wiring.
  try {
    diagnosticsEnabled.value = await isDiagnosticsEnabled();
  } catch (e) {
    console.warn('Failed to load diagnostics setting', e);
  }

  unlistenSignInPrompt = await listen('tray://show-sign-in-prompt', () => {
    signInPrompt.value = true;
  });
  const unTraySignIn = await listen<unknown>('tray://sign-in', (e) => {
    void handleTraySignIn(e.payload);
  });
  // This window is pre-created hidden and outlives sign-ins and sign-outs made
  // elsewhere (Onboarding, the Meetings popover, a rejected session cleared
  // natively), so follow the backend's broadcast rather than its own mount.
  const unAuthChanged = await listen(AUTH_CHANGED_EVENT, async () => {
    await refreshSignedInAccount(true);
    if (isSignedIn.value) signInPrompt.value = false;
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

  unlistenUpdates.push(unTraySignIn, unAuthChanged, unAvail, unNone, unChecking, unError);

  try {
    backend.value = await getBackendSetting();
  } catch (e) {
    console.error('Failed to read backend setting; defaulting to Ariso', e);
  }
  if (backend.value === 'local') await refreshModelStatus();
  await loadNotesModel();
  await loadSpeechModel();
  if (backend.value === 'local') await loadModelSizes();
  await loadVaultDir();

  // Per-model download progress. Completion/failure is handled by the awaited
  // install calls (onInstallStt / onInstallLlm); these events only feed the bar.
  const unSttProgress = await listen<number>('model://stt/progress', (e) => {
    sttProgress.value = e.payload >= 0 ? e.payload : null;
  });
  const unLlmProgress = await listen<number>('model://llm/progress', (e) => {
    llmProgress.value = e.payload >= 0 ? e.payload : null;
  });
  const unBackendChanged = await listen(BACKEND_CHANGED_EVENT, () => {
    void onBackendChangedElsewhere();
  });
  const unModelPrompt = await listen('tray://show-model-prompt', async () => {
    modelPrompt.value = true;
    // The recording gate fired because a model isn't ready — auto-start the
    // missing download(s).
    await refreshModelStatus();
    startMissingDownloads();
  });
  unlistenUpdates.push(unSttProgress, unLlmProgress, unModelPrompt, unBackendChanged);
});

// Registered as its own hook so a failure in the main bootstrap above can't
// prevent the recording guard from arming.
onMounted(async () => {
  void refreshRecordingState();
  window.addEventListener('focus', onWindowFocus);
  const unRecording = await listen<boolean>('recording://state', (e) => {
    recordingActive.value = e.payload;
  });
  unlistenUpdates.push(unRecording);
});

onUnmounted(() => {
  unlistenSignInPrompt?.();
  unlistenUpdates.forEach((un) => un());
  window.removeEventListener('focus', onWindowFocus);
});

async function handleSignIn(provider: SignInProvider) {
  const result = await signIn(provider);
  if (result.error) return;
  signInPrompt.value = false;
  if (provider === 'google') {
    // Sign-in may not carry Calendar — a user who already granted Google
    // Workspace keeps their broader grant, which might not cover it.
    await refreshCalendarAccess();
  } else {
    // Calendar comes from Google only, and the Connect Calendar nudge opens
    // Google's consent page. Clear any verdict left by an earlier Google
    // session in this window so the nudge stays hidden.
    calendarConnected.value = null;
  }
}

/**
 * A "Sign in with …" row in the tray, which surfaces this window first. The
 * tray offers them only while no session is stored, but this window's account
 * state can lag — a session the server rejected is cleared natively without
 * telling it — so re-read it before starting. Dropped while a flow is pending
 * (Cancel here is the way to switch providers) and on Local, which must make no
 * network calls even if the request raced a backend switch.
 */
async function handleTraySignIn(provider: unknown) {
  if (provider !== 'google' && provider !== 'microsoft') return;
  if (backend.value !== 'ariso' || isSigningIn.value) return;
  await refreshSignedInAccount();
  // Checked again after the await: a second tray click may have started a
  // flow meanwhile (handleSignIn marks it pending synchronously), or the
  // backend may have switched to Local while this await was pending.
  if (backend.value !== 'ariso' || isSignedIn.value || isSigningIn.value) return;
  await handleSignIn(provider);
}

/**
 * Acquire Calendar if the API says it is missing. Deliberately not run on mount
 * or window focus: the connect hop opens a browser consent screen, so it fires
 * only on an explicit sign-in or a click of Connect below.
 */
async function refreshCalendarAccess() {
  isConnectingCalendar.value = true;
  calendarConnected.value = null;
  try {
    const status = await auth.ensureCalendarAccess();
    calendarConnected.value = status.connected;
  } catch (err) {
    console.warn('Could not connect Google Calendar', err);
    calendarConnected.value = false;
  } finally {
    isConnectingCalendar.value = false;
  }
}
</script>

<style scoped>
.settings {
  padding: 24px;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  background: #f7f6f4;
  color: #1c1c1c;
  /* Own the full window height and scroll internally so a tall settings stack
     (Local models + Account + Recording + Notifications + About) is reachable
     on short windows instead of being clipped. */
  height: 100vh;
  box-sizing: border-box;
  overflow-y: auto;
  /* Keep scrolling functional but hide the scrollbar chrome so no persistent
     bar shows at rest. */
  scrollbar-width: none; /* Firefox */
}

/* WebKit (the Tauri webview on macOS): hide the scrollbar track/thumb. */
.settings::-webkit-scrollbar {
  width: 0;
  height: 0;
}

.title {
  font-size: 20px;
  font-weight: 700;
  margin-bottom: 24px;
  color: #1c1c1c;
}

.signin-banner {
  background: #f7efdc;
  border: 1px solid #e3d3a8;
  color: #7a5c1e;
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
  background: #e6f2ea;
  border: 1px solid #bfe0cc;
  color: #226741;
}

.notif-status--err {
  background: #f7e7e4;
  border: 1px solid #e0c0ba;
  color: #9c3a2e;
}

.section {
  margin-bottom: 20px;
}

.section-title {
  font-size: 11px;
  font-weight: 600;
  color: #9a9a96;
  text-transform: uppercase;
  letter-spacing: 1.5px;
  margin-bottom: 8px;
}

.card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  padding: 16px;
  box-shadow: 2px 2px 0 #e7e5e2;
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
  background: #1c1c1c;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
  color: white;
  flex-shrink: 0;
  object-fit: cover;
}

.account-details {
  display: flex;
  flex-direction: column;
  flex: 1;
}

.account-name {
  font-size: 14px;
  font-weight: 500;
  color: #1c1c1c;
}

.account-email {
  font-size: 12px;
  color: #6f6f6f;
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

.calendar-connect {
  margin-top: 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.calendar-connect-text {
  font-size: 12px;
  /* The file's muted 12px colour (.account-email, .setting-hint): 5.0:1 on the
     white card, where #9ca3af managed only 2.5:1 — under the WCAG AA floor. */
  color: #6f6f6f;
  margin: 0;
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
  color: #1c1c1c;
}

.model-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.model-table th {
  text-align: left;
  font-weight: 500;
  font-size: 12px;
  color: #6f6f6f;
  padding: 0 8px 8px;
  border-bottom: 1px solid #e5e6e3;
}

.model-table td {
  padding: 10px 8px;
  border-bottom: 1px solid #f1f1ef;
  color: #1c1c1c;
  vertical-align: middle;
}

.model-table tr:last-child td {
  border-bottom: none;
}

.model-row--selectable {
  cursor: pointer;
}

.model-row--selectable:hover td {
  background: rgba(0, 0, 0, 0.03);
}

.model-row--active td {
  background: #f5f5f7;
}

/* The cells stay real table-cells — `display: flex` on a <td> drops it out of
   table layout, so its height stops tracking the rest of the row. The flex row
   lives on an inner span instead. */
.cell-flex {
  display: flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}

.cell-flex--end {
  justify-content: flex-end;
  gap: 12px;
}

.model-name {
  /* Flush with the card's own padding — no extra inset before the name. */
  padding-left: 0;
}

/* Qualified with the table + element selector so it outranks `.model-table th`
   (which sets text-align: left and would otherwise win on specificity). */
.model-table th.model-runtime-head {
  text-align: center;
}

/* `width: 1%` on a full-width table collapses a column to its content width,
   so Type (an icon) and Runtime (size + button) stay tight and Name absorbs
   the remaining space. */
.model-table th.model-type-head,
.model-table td.model-type,
.model-table th.model-runtime-head,
.model-table td.model-runtime {
  width: 1%;
  white-space: nowrap;
}

.model-table th:first-child {
  padding-left: 0;
}

/* Only renders when the model is actually in use. `margin-left: auto` pushes
   it to the far end of the name cell rather than letting it sit against the
   name, so the ticks line up down the column whatever the names are. */
.model-tick {
  display: inline-block;
  flex-shrink: 0;
  margin-left: auto;
  color: #2e8b4f;
}

.model-type {
  color: #6f6f6f;
}

.model-type-icon {
  display: inline-flex;
  cursor: help;
}

.model-type-icon svg {
  width: 16px;
  height: 16px;
}

.model-type-icon:focus-visible + .help-tooltip {
  opacity: 1;
  visibility: visible;
}

/* The Settings window is a fixed 450px and `.settings` clips on both axes
   (it scrolls vertically), so a left-anchored 260px bubble would run off the
   right edge from this middle column. Center it on the icon and narrow it:
   at this width that keeps both edges inside the card. */
.model-type .help-tooltip {
  left: 50%;
  right: auto;
  transform: translateX(-50%);
  width: 200px;
}

/* Icon-only action (delete). The native title supplies the "Delete" tip. */
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid #d6d6d6;
  border-radius: 8px;
  background: #ffffff;
  color: #6f6f6f;
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
}

.icon-btn svg {
  width: 15px;
  height: 15px;
}

.icon-btn:hover:not(:disabled),
.icon-btn:focus-visible:not(:disabled) {
  border-color: #c0392b;
  color: #c0392b;
}

.icon-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.model-runtime {
  white-space: nowrap;
}

.model-size {
  color: #6f6f6f;
  font-variant-numeric: tabular-nums;
}

.model-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}

.model-status {
  font-size: 13px;
  color: #6f6f6f;
}

.model-ready {
  color: #2e8b4f;
  font-size: 16px;
  font-weight: 700;
  line-height: 1;
}

/* The displayed path is front-truncated to <20 chars in JS (leading "..."),
   so the tail stays visible; the full path is available via the `title`
   attribute on hover. Match the "Vault location" label font, not monospace. */
.vault-path {
  white-space: nowrap;
  font-size: 14px;
  color: #1c1c1c;
}

.label-with-help {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.help {
  position: relative;
  display: inline-flex;
}

.help-btn {
  width: 16px;
  height: 16px;
  padding: 0;
  border: 1px solid #d6d6d6;
  border-radius: 50%;
  background: #ffffff;
  color: #6f6f6f;
  font-size: 11px;
  line-height: 1;
  font-family: inherit;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: help;
}

.help-btn:hover,
.help-btn:focus-visible {
  border-color: #1c1c1c;
  color: #1c1c1c;
}

/* Description is revealed only when the "?" is hovered or keyboard-focused. */
.help-tooltip {
  /* Always wrap inside the bubble: a tooltip can sit inside a nowrap cell
     (the models table) and would otherwise inherit it and overflow. */
  white-space: normal;
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 20;
  width: 260px;
  padding: 8px 10px;
  border-radius: 8px;
  background: #1c1c1c;
  color: #ffffff;
  font-size: 12px;
  line-height: 1.4;
  box-shadow: 2px 2px 0 rgba(0, 0, 0, 0.15);
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.12s ease;
  pointer-events: none;
}

.help:hover .help-tooltip,
.help-btn:focus-visible + .help-tooltip {
  opacity: 1;
  visibility: visible;
}

.backend-select {
  position: relative;
}

.backend-trigger {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  padding: 5px 12px;
  border: 1px solid #d6d6d6;
  border-radius: 999px;
  background: #ffffff;
  box-shadow: 2px 2px 0 #e7e5e2;
  color: #1c1c1c;
  font-family: inherit;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}

.backend-trigger:hover:not(:disabled) {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}

.backend-trigger:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.setting-hint {
  margin-top: 8px;
  font-size: 12px;
  color: #6f6f6f;
}

.backend-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

.backend-chevron {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  color: #9a9a96;
}

.backend-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 10;
  min-width: 100%;
  margin: 0;
  padding: 4px;
  list-style: none;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  box-shadow: 2px 2px 0 #e7e5e2;
}

.backend-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 10px;
  border-radius: 999px;
  font-size: 13px;
  color: #1c1c1c;
  white-space: nowrap;
  cursor: pointer;
}

.backend-option:hover {
  background: rgba(0, 0, 0, 0.03);
}

.backend-option:focus-visible {
  background: #f5f5f7;
  outline: 2px solid #6366f1;
  outline-offset: -2px;
}

.backend-option--active {
  background: #1c1c1c;
  color: #ffffff;
}

.backend-option--active:hover,
.backend-option--active:focus-visible {
  background: #1c1c1c;
  color: #ffffff;
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
  color: #1c1c1c;
}

.status-line {
  font-size: 12px;
}

.status-ok       { color: #2e8b4f; }
.status-checking { color: #6f6f6f; }
.status-available { color: #1c1c1c; font-weight: 500; }

.update-controls {
  margin-bottom: 12px;
}

.primary-btn {
  font-size: 13px;
  padding: 6px 14px;
  border-radius: 999px;
  border: none;
  background: #1c1c1c;
  color: white;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
}

.secondary-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 999px;
  border: 1px solid #d6d6d6;
  background: #ffffff;
  box-shadow: 2px 2px 0 #e7e5e2;
  color: #1c1c1c;
  font-family: inherit;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}

.secondary-btn:hover:not(:disabled) {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
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
  color: #1c1c1c;
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
  background: #d6d6d6;
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
  background: #1c1c1c;
}

.toggle-input:checked + .toggle-track .toggle-thumb {
  transform: translateX(16px);
}

.toggle-input:focus-visible + .toggle-track {
  outline: 2px solid #1c1c1c;
  outline-offset: 2px;
}

/* Greyed out when unavailable (e.g. Auto-record while Microphone is off). */
.toggle:has(.toggle-input:disabled) {
  cursor: not-allowed;
}

.toggle-input:disabled + .toggle-track {
  opacity: 0.5;
}

.download-confirm {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.35);
  padding: 24px;
}

.download-confirm__card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  padding: 20px;
  max-width: 360px;
  box-shadow: 2px 2px 0 #e7e5e2;
}

.download-confirm__title {
  font-size: 16px;
  font-weight: 700;
  margin: 0 0 8px;
  color: #1c1c1c;
}

.download-confirm__body {
  font-size: 13px;
  color: #6f6f6f;
  margin: 0 0 16px;
  line-height: 1.5;
}

.download-confirm__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
