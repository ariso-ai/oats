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

    <!-- One confirmation for both kinds of removal: a local model's files, or
         a provider's stored API key. -->
    <div
      v-if="removeTarget || removeKeyProvider"
      class="download-confirm"
      data-test="remove-confirm"
      role="dialog"
      aria-modal="true"
      aria-labelledby="remove-confirm-title"
    >
      <div class="download-confirm__card">
        <h2 id="remove-confirm-title" class="download-confirm__title">
          <template v-if="removeTarget">Remove {{ removeTarget.name }}?</template>
          <template v-else>Remove {{ removeKeyProviderLabel }} API key?</template>
        </h2>
        <p v-if="removeTarget" class="download-confirm__body">
          Its files are deleted from this device. Recording in Local mode needs
          this model, so it has to be downloaded again before the next meeting.
        </p>
        <p v-else class="download-confirm__body">
          The key is deleted from {{ keychainName }}. {{ removeKeyProviderLabel }}
          models can't write notes until you add a key again.
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

    <!-- AI models -->
    <section v-if="backend === 'local'" class="section" data-test="models-section">
      <h2 class="section-title">AI Models</h2>
      <div class="card">
        <div v-if="showModelBanner" class="signin-banner">
          Recording works right away. On-device models are finishing their
          download in the background — transcripts and notes for new
          recordings will be generated once they're ready.
        </div>
        <!-- Sized in rows, not pixels: more models ship than belong on screen
             at once, so the list scrolls inside the card instead of pushing
             the sections below it out of reach. -->
        <div
          class="model-list"
          @mouseenter="onModelListEnter"
          @mouseleave="modelListHovered = false"
        >
        <div
          ref="modelScrollEl"
          class="model-table-scroll"
          data-test="model-scroll"
          :style="{ '--model-visible-rows': MODEL_ROWS_VISIBLE }"
          @scroll="measureModelList"
        >
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
              v-for="row in visibleCatalog"
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
                <button
                  type="button"
                  class="cell-flex"
                  :aria-pressed="isActiveModel(row)"
                  @click.stop="onRowClick(row)"
                >
                  {{ row.name }}
                  <span
                    v-if="isActiveModel(row)"
                    class="model-tick model-tick--on"
                    title="Currently in use"
                    aria-label="Currently in use"
                  >✓</span>
                </button>
              </td>
              <td class="model-type">
                <span class="help">
                  <span
                    class="model-type-icon"
                    data-test="model-type-icon"
                    role="img"
                    tabindex="0"
                    :aria-label="`${modelTypeLabel(row)}. ${row.details}`"
                    @mouseenter="showModelDetails($event, row)"
                    @mouseleave="modelDetails = null"
                    @focus="showModelDetails($event, row)"
                    @blur="modelDetails = null"
                  >
                    <!-- Speech: a microphone. Language: lines of text, or a
                         cloud when the model is a provider's rather than ours. -->
                    <svg v-if="row.type === 'Speech'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                      <line x1="12" y1="19" x2="12" y2="23" />
                    </svg>
                    <svg v-else-if="row.runtime === 'remote'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M17.5 19a4.5 4.5 0 0 0 .3-9 6.5 6.5 0 0 0-12.5 2A4 4 0 0 0 6 19z" />
                      <line x1="8" y1="14" x2="14" y2="14" />
                    </svg>
                    <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                      <polyline points="14 2 14 8 20 8" />
                      <line x1="8" y1="13" x2="16" y2="13" />
                      <line x1="8" y1="17" x2="13" y2="17" />
                    </svg>
                  </span>
                </span>
              </td>
              <td class="model-runtime">
                <span class="cell-flex cell-flex--end">
                <template v-if="row.runtime === 'local'">
                  <!-- Size or status, never both: the size is unknown while a model
                       is downloading (it would only ever be the "—" placeholder) and
                       beside the point when one failed or cannot run here. Ceding the
                       space also keeps those longer messages readable in a column
                       that no longer widens to fit them. -->
                  <span
                    v-if="!rowDetail(row)"
                    class="model-size"
                    data-test="model-size"
                  >{{ rowSize(row) }}</span>
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
                    class="icon-btn icon-btn--install"
                    data-test="install-model"
                    :title="rowBusy(row) === 'downloading' ? 'Downloading' : 'Install'"
                    :aria-label="rowBusy(row) === 'downloading' ? 'Downloading' : 'Install'"
                    :disabled="anyDownloading"
                    @click.stop="onInstallRow(row)"
                  >
                    <!-- Arrow into a tray: download. The label lives in the
                         native tooltip, so the row stays icon-only. -->
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 3v12" />
                      <polyline points="7 10 12 15 17 10" />
                      <path d="M4 19h16" />
                    </svg>
                  </button>
                </template>
                <template v-else>
                  <!-- No "Remote" label: the cloud type icon already says it.
                       Nor a "Connected" one: a provider with a key stored is the
                       row whose plus has become a minus. -->
                  <button
                    v-if="rowConnected(row)"
                    class="icon-btn"
                    data-test="remove-key"
                    title="Remove API key"
                    :aria-label="`Remove ${rowProviderLabel(row)} API key`"
                    @click.stop="onRemoveKey(row)"
                  >
                    <!-- A minus: a key is stored, click to remove it. -->
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M5 12h14" />
                    </svg>
                  </button>
                  <button
                    v-else
                    class="icon-btn"
                    data-test="connect-key"
                    title="Add API key"
                    :aria-label="`Add ${rowProviderLabel(row)} API key`"
                    @click.stop="onConnectRow(row)"
                  >
                    <!-- A plus: add a key. Like Install and Delete, the words
                         live in the native tooltip. -->
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 5v14M5 12h14" />
                    </svg>
                  </button>
                </template>
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        </div>
        <!-- Outside the scroll container on purpose: a bubble rendered inside
             it is clipped by its bottom edge. Fixed-positioned to the icon it
             describes. -->
        <div
          v-if="modelDetails"
          role="tooltip"
          class="help-tooltip help-tooltip--floating"
          data-test="model-details"
          :style="{ left: `${modelDetails.x}px`, top: `${modelDetails.y}px` }"
        >{{ modelDetails.text }}</div>
        <!-- Our own scrollbar: see modelListScrollbar.ts for why the native
             one cannot fade in on hover in this webview. -->
        <div
          ref="modelTrackEl"
          class="model-scrollbar"
          :class="{ 'model-scrollbar--visible': (modelListHovered || draggingThumb) && !!modelThumb }"
          data-test="model-scrollbar"
          aria-hidden="true"
        >
          <div
            v-if="modelThumb"
            class="model-scrollbar__thumb"
            data-test="model-scrollbar-thumb"
            :style="{ height: `${modelThumb.height}px`, transform: `translateY(${modelThumb.offset}px)` }"
            @pointerdown="onThumbPointerDown"
            @pointermove="onThumbPointerMove"
            @pointerup="onThumbPointerUp"
            @pointercancel="onThumbPointerUp"
          />
        </div>
        </div>
        <!-- Asking for a key is a one-at-a-time affair, so the field lives
             under the table rather than inside whichever row started it. -->
        <div v-if="keyProvider" class="key-prompt" data-test="key-prompt">
          <p class="setting-hint" data-test="remote-disclosure">
            Notes for new recordings are sent to {{ keyProviderLabel }}. Recording,
            transcription, and audio stay on this device.
          </p>
          <div class="key-prompt__row">
            <input
              v-model="keyInput"
              class="key-input"
              data-test="api-key-input"
              type="password"
              autocomplete="off"
              spellcheck="false"
              :placeholder="`${keyProviderLabel} API key`"
              :aria-label="`${keyProviderLabel} API key`"
              @keyup.enter="onSaveKey"
            />
            <button
              class="primary-btn"
              data-test="save-key"
              :disabled="savingKey"
              @click="onSaveKey"
            >
              Save
            </button>
            <button class="secondary-btn" @click="cancelKeyPrompt">Cancel</button>
          </div>
          <p class="setting-hint">
            Stored in your {{ keychainName }}, never in oats' settings file.
          </p>
          <p
            v-if="keyError"
            class="setting-hint"
            data-test="key-error"
            style="color: var(--danger, #c0392b)"
          >
            {{ keyError }}
          </p>
        </div>
        <p v-if="remoteModelInUse" class="setting-hint" data-test="remote-pending">
          Notes are still written on this device — the selected remote model starts
          writing them in a later update.
        </p>
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
import { AUTH_CHANGED_EVENT, auth, updater, getBackendSetting, setBackendSetting, hasPromptedLocalModels, setPromptedLocalModels, getNotesModelSetting, setNotesModelSetting, getSpeechModelSetting, setSpeechModelSetting, local, llmKeys, getVaultDir, setVaultDir, pickVaultFolder, type ModelStatus, type ModelSizes, type LocalModelKind, type SpeechModelId, type SttProgress } from '../tauri';
import { DEFAULT_NOTES_MODEL, notesModelKey, remoteProviderLabel, type NotesModelId, type RemoteProvider } from '../notesModels';
import { modelCatalog, formatModelSize, speechModelIdFromKey, DEFAULT_SPEECH_MODEL_KEY, type CatalogModel } from '../modelCatalog';
import { thumbGeometry, scrollTopForDrag, type ScrollMetrics } from './modelListScrollbar';
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

// Per-model download UI state — every speech model's Install button, and the
// LLM's, are independent of one another.
const speechBusy = ref<Partial<Record<SpeechModelId, Busy>>>({});
const speechProgress = ref<Partial<Record<SpeechModelId, number | null>>>({});
const llmBusy = ref<Busy>('idle');
const llmProgress = ref<number | null>(null);
/** The speech model currently selected to transcribe. */
const selectedSpeechId = computed(() => speechModelIdFromKey(speechModelKey.value));
const busyOf = (id: SpeechModelId): Busy => speechBusy.value[id] ?? 'idle';

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

// --- AI models table (Local backend only) ----------------------------------
// The table is the single control: clicking a Notes row makes that model the
// one that writes notes, and each local row carries its own install button.
const notesModel = ref<NotesModelId>(DEFAULT_NOTES_MODEL);
const catalog = modelCatalog();

/** Hides a speech row this platform doesn't offer (e.g. Qwen3 on Windows).
 *  Before `modelStatus.speech` has answered, nothing is hidden yet. */
const visibleCatalog = computed(() => {
  const offered = modelStatus.value.speech?.map((s) => s.id);
  return offered
    ? catalog.filter((row) => row.type !== 'Speech' || offered.includes(row.speechModel!))
    : catalog;
});

/** How many rows the list shows before it scrolls. */
const MODEL_ROWS_VISIBLE = 6;

/** Whether the pointer is over the list, which is what reveals its scrollbar. */
const modelListHovered = ref(false);
const modelScrollEl = ref<HTMLElement | null>(null);
const modelTrackEl = ref<HTMLElement | null>(null);
/** The bar's own height. It starts below the sticky header, so it is shorter
 *  than the scroller's viewport and the thumb must be sized against it. */
const modelTrackHeight = ref(0);
const modelScrollMetrics = ref<ScrollMetrics>({
  scrollTop: 0,
  scrollHeight: 0,
  clientHeight: 0,
});

/** Where the thumb sits, or null while everything fits on screen. */
const modelThumb = computed(() =>
  thumbGeometry(modelScrollMetrics.value, modelTrackHeight.value || undefined),
);

/** The hovered model's description, pinned to the icon it belongs to. */
const modelDetails = ref<{ text: string; x: number; y: number } | null>(null);

/** Pinned to the viewport, so anything that moves the icon leaves the bubble
 *  behind: drop it instead. Capture phase catches the Settings page's own
 *  scroller as well as the window's. */
function dropModelDetails() {
  modelDetails.value = null;
}

onMounted(() => window.addEventListener('scroll', dropModelDetails, true));
onUnmounted(() => window.removeEventListener('scroll', dropModelDetails, true));

function showModelDetails(event: Event, row: CatalogModel) {
  const icon = event.currentTarget as HTMLElement | null;
  if (!icon) return;
  const rect = icon.getBoundingClientRect();
  modelDetails.value = {
    text: row.details,
    x: rect.left + rect.width / 2,
    y: rect.top,
  };
}

function measureModelList() {
  // The bubble is pinned to where the icon was; scrolling moves the icon out
  // from under it.
  modelDetails.value = null;
  const el = modelScrollEl.value;
  if (!el) return;
  modelScrollMetrics.value = {
    scrollTop: el.scrollTop,
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
  };
  modelTrackHeight.value = modelTrackEl.value?.clientHeight ?? 0;
}

// Dragging the thumb scrolls the list, the way a real scrollbar does.
const draggingThumb = ref(false);
let dragStartY = 0;
let dragStartScrollTop = 0;

function onThumbPointerDown(event: PointerEvent) {
  const el = modelScrollEl.value;
  if (!el) return;
  draggingThumb.value = true;
  dragStartY = event.clientY;
  dragStartScrollTop = el.scrollTop;
  // Capture so the drag survives the pointer leaving the 6px-wide thumb, which
  // it does almost immediately.
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  // Don't let the gesture start a text selection across the rows.
  event.preventDefault();
}

function onThumbPointerMove(event: PointerEvent) {
  const el = modelScrollEl.value;
  const thumb = modelThumb.value;
  if (!draggingThumb.value || !el || !thumb) return;
  el.scrollTop = scrollTopForDrag({
    startScrollTop: dragStartScrollTop,
    deltaY: event.clientY - dragStartY,
    metrics: modelScrollMetrics.value,
    thumbHeight: thumb.height,
    trackLength: modelTrackHeight.value || undefined,
  });
  measureModelList();
}

function onThumbPointerUp(event: PointerEvent) {
  draggingThumb.value = false;
  const target = event.currentTarget as HTMLElement;
  if (target.hasPointerCapture(event.pointerId)) {
    target.releasePointerCapture(event.pointerId);
  }
}

function onModelListEnter() {
  modelListHovered.value = true;
  // Rows arrive and depart (a download finishes, a key is removed), so measure
  // when the pointer shows up rather than trusting a stale measurement.
  measureModelList();
}

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
 *  model transcribes, one notes model writes the notes. Holding the selection
 *  is not enough — a local model that is not on disk cannot run, so a deleted
 *  model loses its tick even though it stays selected, and with no other model
 *  of that type installed the column shows no tick at all. */
/** Whether this model could run right now: downloaded, for an on-device one;
 *  a key stored for its provider, for a remote one. Only a usable model can be
 *  the one in use. */
function rowUsable(row: CatalogModel): boolean {
  return row.runtime === 'remote' ? rowConnected(row) : rowInstalled(row);
}

function isActiveModel(row: CatalogModel): boolean {
  if (!rowUsable(row)) return false;
  if (row.type === 'Speech') return row.key === speechModelKey.value;
  return (
    !!row.notesModel && notesModelKey(row.notesModel) === notesModelKey(notesModel.value)
  );
}

/** What the type icon announces. A remote model is called out here rather than
 *  with a word in the row — the cloud icon carries it visually, this carries it
 *  for a screen reader and the hover tip. */
function modelTypeLabel(row: CatalogModel): string {
  if (row.type === 'Speech') return 'Speech model';
  return row.runtime === 'remote' ? 'Remote language model' : 'Language model';
}

/** A speech model's own readiness. Before a backend that reports per-model
 *  status answers, only the selected model's overall state is known. */
function speechReady(id: SpeechModelId): boolean {
  const entry = modelStatus.value.speech?.find((s) => s.id === id);
  if (entry) return entry.ready;
  return id === selectedSpeechId.value && modelStatus.value.state === 'ready';
}

/** Speech rows read their own model's download state; every other row is the
 *  notes model. */
function rowInstalled(row: CatalogModel): boolean {
  return row.type === 'Speech' ? speechReady(row.speechModel!) : llmInstalled.value;
}

function rowBusy(row: CatalogModel): Busy {
  return row.type === 'Speech' ? busyOf(row.speechModel!) : llmBusy.value;
}

/** Text beside the size: progress while downloading, and failures — but not
 *  "not downloaded", which the missing tick and the Install button already say. */
function rowDetail(row: CatalogModel): string {
  const progress = row.type === 'Speech' ? speechProgress.value[row.speechModel!] ?? null : llmProgress.value;
  return rowDetailText(rowBusy(row), progress, rowInstalled(row), unsupported.value);
}

const modelSizes = ref<ModelSizes>({ notes: null, speech: {} });
const removeTarget = ref<CatalogModel | null>(null);
const removeError = ref('');

async function loadModelSizes() {
  try {
    modelSizes.value = await local.modelSizes();
  } catch (e) {
    console.error('Failed to read model sizes', e);
    modelSizes.value = { notes: null, speech: {} };
  }
}

function rowKind(row: CatalogModel): LocalModelKind {
  return row.type === 'Speech' ? 'speech' : 'notes';
}

function rowSize(row: CatalogModel): string {
  if (row.type === 'Speech') {
    return formatModelSize(modelSizes.value.speech[row.speechModel!] ?? null);
  }
  return formatModelSize(modelSizes.value.notes);
}

function onRemoveRow(row: CatalogModel) {
  removeError.value = '';
  removeTarget.value = row;
}

function cancelRemove() {
  removeTarget.value = null;
  removeKeyProvider.value = null;
}

async function confirmRemove() {
  if (removeKeyProvider.value) {
    await confirmRemoveKey();
    return;
  }
  const row = removeTarget.value;
  removeTarget.value = null;
  if (!row) return;
  let removed = false;
  try {
    await local.deleteModel(rowKind(row), row.speechModel);
    removed = true;
  } catch (e) {
    // The backend refuses mid-recording and mid-download; say which, rather
    // than leaving the row looking installed for no stated reason.
    removeError.value = e instanceof Error ? e.message : String(e);
  }
  await refreshModelStatus();
  if (removed && row.type === 'Speech' && row.key === speechModelKey.value) {
    await selectInstalledSpeechFallback();
  }
  await loadModelSizes();
}

/** Removing the speech model in use would otherwise leave the selection on a
 *  model that is gone: recording stays blocked and the tray's download prompt
 *  re-fetches what was just removed. Another installed one takes over. */
async function selectInstalledSpeechFallback() {
  const fallback = visibleCatalog.value.find(
    (r) => r.type === 'Speech' && r.key !== speechModelKey.value && speechReady(r.speechModel!)
  );
  if (!fallback) return;
  try {
    await setSpeechModelSetting(fallback.key);
    speechModelKey.value = fallback.key;
  } catch (e) {
    console.error('Failed to persist speech model', e);
    return;
  }
  // Overall readiness follows the selected model.
  await refreshModelStatus();
}

function onInstallRow(row: CatalogModel) {
  if (row.runtime !== 'local') return;
  if (row.type === 'Speech') void onInstallSpeech(row.speechModel!);
  else void onInstallLlm();
}

// --- Remote provider API keys ----------------------------------------------
// A key lives in the OS keychain, reachable only from Rust; the webview learns
// which providers have one and nothing more.
const connectedProviders = ref<RemoteProvider[]>([]);
const keyProvider = ref<RemoteProvider | null>(null);
const keyInput = ref('');
const keyError = ref('');
const savingKey = ref(false);

const keyProviderLabel = computed(() =>
  keyProvider.value ? remoteProviderLabel(keyProvider.value) : '',
);
/** The caveat belongs to the model actually in use, not to every stored key:
 *  connecting a provider you haven't selected changes nothing about notes. */
const remoteModelInUse = computed(() => notesModel.value.kind === 'remote');
const keychainName = computed(() =>
  platformCapabilities.value.os === 'windows' ? 'Windows Credential Manager' : 'macOS Keychain',
);

async function loadConnectedProviders() {
  try {
    connectedProviders.value = await llmKeys.providers();
  } catch (e) {
    console.error('Failed to read stored API keys', e);
    connectedProviders.value = [];
  }
}

function rowProvider(row: CatalogModel): RemoteProvider | null {
  return row.notesModel?.kind === 'remote' ? row.notesModel.provider : null;
}

function rowProviderLabel(row: CatalogModel): string {
  const provider = rowProvider(row);
  return provider ? remoteProviderLabel(provider) : '';
}

/** One key per provider, so every model behind it reads as connected. */
function rowConnected(row: CatalogModel): boolean {
  const provider = rowProvider(row);
  return !!provider && connectedProviders.value.includes(provider);
}

function onConnectRow(row: CatalogModel) {
  const provider = rowProvider(row);
  if (!provider) return;
  keyProvider.value = provider;
  keyInput.value = '';
  keyError.value = '';
}

function cancelKeyPrompt() {
  keyProvider.value = null;
  keyInput.value = '';
  keyError.value = '';
}

async function onSaveKey() {
  const provider = keyProvider.value;
  if (!provider || savingKey.value) return;
  keyError.value = '';
  savingKey.value = true;
  try {
    await llmKeys.set(provider, keyInput.value);
    // The backend accepted the write, so the provider is connected; its value
    // is never read back to confirm it.
    if (!connectedProviders.value.includes(provider)) {
      connectedProviders.value = [...connectedProviders.value, provider];
    }
    cancelKeyPrompt();
  } catch (e) {
    // Keep the field (and the unsaved key) in place so a rejected paste can be
    // corrected rather than retyped.
    keyError.value = e instanceof Error ? e.message : String(e);
  } finally {
    savingKey.value = false;
  }
}

/** The provider whose key the removal dialog is asking about. */
const removeKeyProvider = ref<RemoteProvider | null>(null);
const removeKeyProviderLabel = computed(() =>
  removeKeyProvider.value ? remoteProviderLabel(removeKeyProvider.value) : '',
);

/** Removing a key is asked about first, the same way removing a model is. */
function onRemoveKey(row: CatalogModel) {
  const provider = rowProvider(row);
  if (!provider) return;
  keyError.value = '';
  removeKeyProvider.value = provider;
}

async function confirmRemoveKey() {
  const provider = removeKeyProvider.value;
  removeKeyProvider.value = null;
  if (!provider) return;
  try {
    await llmKeys.clear(provider);
    connectedProviders.value = connectedProviders.value.filter((p) => p !== provider);
  } catch (e) {
    keyError.value = e instanceof Error ? e.message : String(e);
  }
}

async function onRowClick(row: CatalogModel) {
  // A model can only be put into use once it can actually run: downloaded for
  // an on-device one, a key stored for a remote one. A click on a row that
  // isn't there yet asks for the missing half instead of selecting it.
  if (!rowUsable(row)) {
    if (row.runtime === 'remote') onConnectRow(row);
    return;
  }
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
  await loadModelSizes();
  await loadConnectedProviders();
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
  void onInstallSpeech();
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

async function onInstallSpeech(id: SpeechModelId = selectedSpeechId.value) {
  speechBusy.value = { ...speechBusy.value, [id]: 'downloading' };
  speechProgress.value = { ...speechProgress.value, [id]: null };
  try {
    await local.downloadStt(id);
    await refreshModelStatus();
    await loadModelSizes();
    speechBusy.value = { ...speechBusy.value, [id]: 'idle' };
  } catch (e) {
    console.error('Speech model download failed', e);
    speechBusy.value = { ...speechBusy.value, [id]: 'error' };
  }
}

async function onInstallLlm() {
  llmBusy.value = 'downloading';
  llmProgress.value = null;
  try {
    await local.downloadLlm();
    await refreshModelStatus();
    await loadModelSizes();
    llmBusy.value = 'idle';
  } catch (e) {
    console.error('LLM model download failed', e);
    llmBusy.value = 'error';
  }
}

// Kick off downloads for whichever on-device models are still missing. Shared
// by the backend switch and the background download-nudge fired whenever a
// local recording starts while a model is missing. Reads the current
// modelStatus, so callers refresh it first. The Rust per-target guards de-dupe,
// so calling this while a download is already in progress is a safe no-op.
function startMissingDownloads() {
  const pending = pendingInstalls(modelStatus.value, busyOf(selectedSpeechId.value), llmBusy.value);
  if (pending.stt) void onInstallSpeech();
  if (pending.llm) void onInstallLlm();
}

const unsupported = computed(() => modelStatus.value.state === 'unsupported');
// This value controls availability copy and interaction only; OS permission is
// a separate concern handled when the user actually enables capture.
const systemAudioSupported = computed(() => platformCapabilities.value.systemAudio.supported);
// The selected speech model's readiness — `state` reflects it directly.
const sttInstalled = computed(() => modelStatus.value.state === 'ready');
const llmInstalled = computed(() => modelStatus.value.llmReady === true);
const anyDownloading = computed(
  () =>
    Object.values(speechBusy.value).some((b) => b === 'downloading') ||
    llmBusy.value === 'downloading',
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
  // Ariso generates notes server-side, so it needs no provider keys — and must
  // not touch the keychain to find that out.
  if (backend.value === 'local') await loadConnectedProviders();
  await loadVaultDir();

  // Per-model download progress. Completion/failure is handled by the awaited
  // install calls (onInstallSpeech / onInstallLlm); these events only feed the bar.
  const unSttProgress = await listen<SttProgress>('model://stt/progress', (e) => {
    const { model, fraction } = e.payload;
    speechProgress.value = { ...speechProgress.value, [model]: fraction >= 0 ? fraction : null };
  });
  const unLlmProgress = await listen<number>('model://llm/progress', (e) => {
    llmProgress.value = e.payload >= 0 ? e.payload : null;
  });
  const unBackendChanged = await listen(BACKEND_CHANGED_EVENT, () => {
    void onBackendChangedElsewhere();
  });
  const unModelPrompt = await listen('tray://show-model-prompt', async () => {
    modelPrompt.value = true;
    // A local recording just started (or auto-record fired) while a model
    // isn't ready — auto-start the missing download(s) in the background.
    // Recording itself was never blocked on this.
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

/* `table-layout: fixed` so the column widths below are binding. Under the
   default `auto`, a width is only a suggestion — the column still grows to its
   widest content, which is exactly the jumping this is meant to stop. */
/* Row height (a 28px icon button, 10px of padding either side, and the 1px
   rule) and the header's own height — both measured in the running app — so
   the box is sized in whole rows and the sixth one is never clipped. */
.model-table-scroll {
  max-height: calc(var(--model-visible-rows, 6) * var(--model-row-height) + var(--model-head-height));
  overflow-y: auto;
  /* The sticky header needs a positioned scroll container of its own. */
  position: relative;
  /* The wrapper reaches the card's edge (see .model-list); the rows keep their
     inset so the table's right edge lands where it always did. */
  padding-right: 16px;
  /* No `scrollbar-width` here, deliberately: setting it to any value (even
     `thin`) puts this webview's scroller in legacy mode — a permanent 13-17px
     bar that also ignores the ::-webkit-scrollbar width below. Measured in the
     running app: plain scroller 17px, `thin` 13px, pseudo-element only 6px. */
}

/* An overlay-style bar of our own: this webview draws a persistent scrollbar
   for inner scrollers whatever macOS's Show-scroll-bars setting says, so the
   thumb is transparent until the pointer is over the list. The 6px gutter is
   reserved either way, so revealing it never shifts the rows. */
/* The native bar is hidden outright; `.model-scrollbar` below replaces it. */
.model-table-scroll::-webkit-scrollbar {
  width: 0;
  height: 0;
}

.model-list {
  /* Shared by the scroller's max-height and the track's inset, so the bar can
     never disagree with the rows about where the list starts. */
  --model-row-height: 49px;
  --model-head-height: 27px;
  position: relative;
  /* Past the card's 16px padding, so the list's right edge — and the scrollbar
     pinned to it — is the section's own edge. */
  margin-right: -16px;
}

/* Sits in the card's own right padding, so it overlays nothing and shifts no
   rows when it appears. */
.model-scrollbar {
  position: absolute;
  top: var(--model-head-height);
  right: 0;
  bottom: 0;
  width: 6px;
  opacity: 0;
  transition: opacity 120ms ease;
  /* The track stays inert so it never steals a click meant for a row; the
     thumb takes pointer events back so it can be dragged. */
  pointer-events: none;
}

.model-scrollbar--visible {
  opacity: 1;
}

.model-scrollbar__thumb {
  width: 100%;
  border-radius: 3px;
  background: #c9c9c9;
  pointer-events: auto;
  cursor: default;
  /* A drag that wanders off the thumb must not select the rows behind it. */
  user-select: none;
  touch-action: none;
}

.model-scrollbar__thumb:hover {
  background: #b0b0b0;
}

.model-table {
  width: 100%;
  table-layout: fixed;
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
  /* Stays put while the rows scroll under it. Opaque, or rows show through. */
  position: sticky;
  top: 0;
  z-index: 1;
  background: #ffffff;
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
  gap: 8px;
}

/* The name cell's flex row is a real <button> for keyboard access — strip the
   native button chrome so it still reads as plain row text. */
button.cell-flex {
  background: none;
  border: none;
  padding: 0;
  margin: 0;
  font: inherit;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.model-name {
  /* Flush with the card's own padding — no extra inset before the name. */
  padding-left: 0;
}

/* The tick rides the right edge of the Name column rather than trailing the
   text, so it lands in the same place on every row instead of wherever that
   row's name happens to end. */
.model-name button.cell-flex {
  width: 100%;
}

.model-name .model-tick {
  margin-left: auto;
  padding-left: 8px;
}

/* Qualified with the table + element selector so it outranks `.model-table th`
   (which sets text-align: left and would otherwise win on specificity). */
.model-table th.model-runtime-head {
  text-align: center;
}

/* Wide enough for the "Type" header itself (~30px at 12px, plus the cell's
   8px padding either side) — at the icon's own 32px the label was clipped.
   Name carries no width and absorbs whatever these two leave. */
.model-table th.model-type-head,
.model-table td.model-type {
  width: 52px;
  white-space: nowrap;
}

/* Runtime is pinned instead of content-sized: its text changes while a model
   downloads ("Starting…" → "9%" → "90%"), and a content-sized column would
   resize on every tick, dragging the Type column sideways with it. 134px is
   what is left once Name fits its longest entry ("Parakeet TDT 0.6B v3" plus
   its in-use tick) in this fixed-width window — enough for size + status +
   the icon button, with the long status strings wrapping instead. */
.model-table th.model-runtime-head,
.model-table td.model-runtime {
  width: 134px;
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
/* Positioned against the viewport, so no ancestor's overflow can clip it, and
   drawn above the icon it describes. */
.help-tooltip.help-tooltip--floating {
  position: fixed;
  top: 0;
  left: 0;
  width: 200px;
  transform: translate(-50%, calc(-100% - 8px));
  opacity: 1;
  visibility: visible;
}

/* Icon-only row actions (delete, install). The native title supplies the tip,
   so neither button spells its verb out in the row. */
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  /* Never let a long status beside it shrink the hit target: as a flex item it
     would otherwise compress well below 28px. */
  flex-shrink: 0;
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

/* Install is not destructive, so it does not take the delete button's red. */
.icon-btn--install:hover:not(:disabled),
.icon-btn--install:focus-visible:not(:disabled) {
  border-color: #1c1c1c;
  color: #1c1c1c;
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

/* The one thing in this column that changes on its own, so it gets a slot at
   least as wide as its widest progress text ("Starting…"). Every percentage is
   narrower, so the slot does not resize as the download advances — the text
   stays pinned to the same right edge the size occupies when idle. The longer
   one-shot states ("Download failed", "Unsupported on this platform") exceed
   the slot and wrap instead of widening the column. */
.model-status {
  min-width: 54px;
  text-align: right;
  white-space: normal;
  font-size: 13px;
  color: #6f6f6f;
  font-variant-numeric: tabular-nums;
}

/* The key field sits under the table, so it gets the table's own gutter. */
.key-prompt {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid #ececec;
}

.key-prompt__row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}

.key-input {
  flex: 1;
  min-width: 0;
  padding: 6px 8px;
  border: 1px solid #d6d6d6;
  border-radius: 6px;
  background: #ffffff;
  font-family: inherit;
  font-size: 13px;
  color: #1c1c1c;
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
