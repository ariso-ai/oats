<template>
  <div class="update-window" :class="{ 'is-current': !hasUpdate }">
    <header class="update-hero">
      <img class="app-icon" src="../assets/oats-light.svg" alt="Ariso" />

      <div class="hero-copy">
        <h1 class="title">{{ titleText }}</h1>
        <p v-if="hasUpdate && updateInfo" class="subtitle">
          Version <span class="version-number">{{ updateInfo.version }}</span> is
          ready. You have {{ currentVersion }}.
        </p>
        <p v-else class="subtitle">
          Version <span class="version-number">{{ currentVersion }}</span> is
          installed.
        </p>
      </div>
    </header>

    <main class="update-content" :class="{ 'is-current': !hasUpdate }">
      <section
        v-if="hasUpdate"
        class="release-notes-section"
        aria-label="Release notes"
      >
        <div
          v-if="releaseNotesHtml"
          class="release-notes"
          v-html="releaseNotesHtml"
        />
        <p v-else class="notes-empty">
          Release notes aren&rsquo;t available for this update.
        </p>
      </section>

      <section
        v-else
        class="current-section"
        aria-labelledby="current-version"
      >
        <CheckCircleIcon class="status-icon" aria-hidden="true" />
        <div>
          <h2 id="current-version" class="current-title">
            You&rsquo;re on the latest version
          </h2>
          <p>Oats {{ currentVersion }} is installed.</p>
        </div>
      </section>

      <section v-if="hasUpdate" class="benefit-section" aria-label="Update benefit">
        <MicrophoneIcon class="mic-icon" aria-hidden="true" />
        <p>Keeps recording and transcription improvements up to date</p>
      </section>
    </main>

    <div v-if="downloadState === 'idle' && downloadError" class="error-line">
      {{ downloadError }}
    </div>

    <div v-if="downloadState === 'downloading'" class="progress-row">
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: progressPct + '%' }"></div>
      </div>
      <div class="progress-label">{{ progressPct }}%</div>
    </div>

    <footer v-if="downloadState === 'idle'" class="actions">
      <div v-if="hasUpdate" class="left-actions">
        <a
          v-if="!updateInfo?.mandatory"
          href="#"
          @click.prevent="onSkip"
          class="link-action"
        >Skip</a>
        <a
          v-if="!updateInfo?.mandatory"
          href="#"
          @click.prevent="onLater"
          class="link-action"
        >Later</a>
      </div>
      <button
        v-if="hasUpdate"
        class="install-btn"
        @click="onInstall"
      >Install Update</button>
      <button v-else class="done-btn" @click="onDone">Done</button>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { CheckCircleIcon, MicrophoneIcon } from '@heroicons/vue/24/outline';
import { updater, type UpdateInfo } from '../tauri';
import { renderMarkdown } from '../utils/markdown';

const currentVersion = __APP_VERSION__;

const updateInfo = ref<UpdateInfo | null>(null);
const downloadState = ref<'idle' | 'downloading'>('idle');
const downloadError = ref('');
const downloaded = ref(0);
const total = ref<number | null>(null);

const progressPct = computed(() => {
  if (!total.value || total.value === 0) return 0;
  return Math.min(100, Math.floor((downloaded.value / total.value) * 100));
});

const hasUpdate = computed(() => {
  const version = updateInfo.value?.version.trim();
  return Boolean(version && version !== currentVersion);
});

const titleText = computed(() =>
  hasUpdate.value ? 'What’s new for Oats?' : 'Oats is up to date'
);

const releaseNotesHtml = computed(() =>
  updateInfo.value?.notes.trim() ? renderMarkdown(updateInfo.value.notes) : ''
);

let unlistenProgress: UnlistenFn | null = null;
let unlistenAvailable: UnlistenFn | null = null;
let unlistenNone: UnlistenFn | null = null;

onMounted(async () => {
  // Initial state (covers the case where the window is opened from
  // Settings → "Show Details" after the event already fired).
  try {
    const snap = await updater.getState();
    if (snap.latest_known) {
      updateInfo.value = snap.latest_known;
    }
  } catch (e) {
    if (!isBrowserPreviewError(e)) {
      downloadError.value = e instanceof Error ? e.message : String(e);
    }
  }

  // Stay in sync if a fresh check fires while we're open.
  unlistenAvailable = await listen<UpdateInfo>('update://available', (e) => {
    updateInfo.value = e.payload;
  });

  unlistenNone = await listen('update://none', () => {
    updateInfo.value = null;
  });

  unlistenProgress = await listen<{ downloaded: number; total: number | null }>(
    'update://download-progress',
    (e) => {
      downloaded.value = e.payload.downloaded;
      total.value = e.payload.total;
    }
  );
});

onUnmounted(() => {
  unlistenProgress?.();
  unlistenAvailable?.();
  unlistenNone?.();
});

async function onInstall() {
  downloadError.value = '';
  downloadState.value = 'downloading';
  downloaded.value = 0;
  total.value = null;
  try {
    await updater.installAndRelaunch();
    // App restarts; this never returns.
  } catch (e) {
    downloadState.value = 'idle';
    downloadError.value =
      e instanceof Error ? e.message : 'Download interrupted. Try again?';
  }
}

async function onSkip() {
  downloadError.value = '';
  if (!updateInfo.value?.version) return;
  try {
    await updater.skipVersion(updateInfo.value.version);
    await getCurrentWindow().close();
  } catch (e) {
    downloadError.value = e instanceof Error ? e.message : String(e);
  }
}

async function onLater() {
  downloadError.value = '';
  try {
    await updater.snooze();
    await getCurrentWindow().close();
  } catch (e) {
    downloadError.value = e instanceof Error ? e.message : String(e);
  }
}

// Closes the informational up-to-date dialog. This is separate from Skip/Later
// because no update version exists when the user is already current.
async function onDone() {
  await getCurrentWindow().close();
}

// Plain browser previews do not have Tauri IPC, so missing invoke errors are
// treated as "current" instead of filling the release rail with fake bullets.
function isBrowserPreviewError(error: unknown): boolean {
  return error instanceof TypeError && error.message.includes('invoke');
}
</script>

<style scoped>
:global(html),
:global(body),
:global(#app) {
  width: 100%;
  height: 100%;
  margin: 0;
  overflow: hidden;
}

.update-window {
  /* Keep the update dialog on the same type and color tokens as Settings. */
  background: #f7f6f4;
  padding: 0;
  box-sizing: border-box;
  width: 100%;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  min-height: 100vh;
  max-height: 100vh;
  display: flex;
  flex-direction: column;
  color: #1c1c1c;
  overflow: hidden;
}

.update-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 46px 36px 15px;
  text-align: center;
  flex: 0 0 auto;
}

.app-icon {
  width: 88px;
  height: auto;
  object-fit: contain;
  flex: 0 0 auto;
}

.hero-copy {
  min-width: 0;
  margin-top: -10px;
}

.title {
  font-size: 24px;
  line-height: 1.12;
  font-weight: 700;
  color: #1c1c1c;
  margin: 0 0 8px 0;
  letter-spacing: 0;
}

.subtitle {
  margin: 0;
  font-size: 15px;
  line-height: 1.35;
  font-weight: 400;
  color: #6f6f6f;
}

.version-number {
  display: inline-block;
  margin-right: 0.16em;
  font-variant-numeric: tabular-nums;
}

.update-content {
  width: calc(100% - 72px);
  max-width: 448px;
  margin: 0 auto;
  border-top: 1px solid #d6d6d6;
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.release-notes-section {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  padding: 22px 12px 16px 8px;
}

.release-notes-section::-webkit-scrollbar {
  width: 8px;
}

.release-notes-section::-webkit-scrollbar-track {
  background: transparent;
}

.release-notes-section::-webkit-scrollbar-thumb {
  background: #d6d6d6;
  border-radius: 999px;
}

.release-notes-section::-webkit-scrollbar-thumb:hover {
  background: #c4c4bf;
}

.release-notes {
  font-size: 15px;
  line-height: 1.35;
  font-weight: 400;
  color: #1c1c1c;
}

.release-notes :deep(h1),
.release-notes :deep(h2),
.release-notes :deep(h3),
.release-notes :deep(h4),
.release-notes :deep(h5),
.release-notes :deep(h6) {
  margin: 0 0 14px;
  color: #1c1c1c;
  font-weight: 600;
  line-height: 1.2;
}

.release-notes :deep(h1) { font-size: 18px; }
.release-notes :deep(h2) { font-size: 17px; }
.release-notes :deep(h3) { font-size: 16px; }
.release-notes :deep(h4),
.release-notes :deep(h5),
.release-notes :deep(h6) { font-size: 15px; }

.release-notes :deep(p) {
  margin: 0 0 12px;
  color: #6f6f6f;
}

.release-notes :deep(ul),
.release-notes :deep(ol) {
  margin: 0 0 12px;
  padding-left: 22px;
}

.release-notes :deep(ul) {
  list-style-type: disc;
}

.release-notes :deep(ol) {
  list-style-type: decimal;
}

.release-notes :deep(li) {
  display: list-item;
  list-style-position: outside;
  line-height: 1.35;
  margin-bottom: 9px;
}

.release-notes :deep(li::marker) {
  color: #ffc20a;
}

.release-notes :deep(a) {
  color: inherit;
  text-decoration: underline;
  text-decoration-color: #c7c7c2;
  text-underline-offset: 2px;
}

.release-notes :deep(code) {
  background: #f0eeed;
  border-radius: 4px;
  padding: 1px 5px;
  font-size: 0.9em;
}

.release-notes :deep(*:last-child) {
  margin-bottom: 0;
}

.notes-empty {
  margin: 0;
  color: #6f6f6f;
  font-size: 14px;
  font-weight: 400;
  line-height: 1.35;
}

.benefit-section {
  display: grid;
  grid-template-columns: 46px 1fr;
  gap: 18px;
  align-items: center;
  border-top: 1px solid #d6d6d6;
  flex: 0 0 auto;
  padding: 18px 10px 16px;
}

.current-section {
  min-height: 176px;
  display: grid;
  grid-template-columns: 50px 1fr;
  gap: 18px;
  align-items: center;
  padding: 30px 10px;
}

.current-section p {
  margin: 0;
  color: #6f6f6f;
  font-size: 14px;
  line-height: 1.35;
  font-weight: 400;
}

.current-title {
  margin: 0 0 8px;
  font-size: 18px;
  line-height: 1.2;
  font-weight: 700;
  color: #1c1c1c;
}

.status-icon {
  width: 36px;
  height: 36px;
  color: #14a34a;
  stroke-width: 1.9;
}

.mic-icon {
  width: 36px;
  height: 36px;
  fill: none;
  stroke: #6f6f6f;
  stroke-width: 2.4;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.benefit-section p {
  margin: 0;
  color: #6f6f6f;
  font-size: 14px;
  line-height: 1.35;
  font-weight: 400;
}

.error-line {
  margin: 12px 48px 0;
  font-size: 12px;
  color: #b42318;
  text-align: center;
  font-weight: 500;
}

.progress-row {
  margin-top: 14px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.progress-track {
  flex: 1;
  height: 7px;
  background: #e1e5ec;
  border-radius: 999px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: #ffcb14;
  transition: width 0.2s;
}

.progress-label {
  font-size: 11px;
  color: #6f7785;
  width: 32px;
  text-align: right;
  font-weight: 600;
}

.actions {
  margin-top: 0;
  width: 100%;
  box-sizing: border-box;
  align-self: stretch;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
  flex: 0 0 auto;
  min-height: 78px;
  padding: 0 32px 0 36px;
  background: rgba(247, 246, 244, 0.96);
  border-top: 1px solid #d6d6d6;
}

.left-actions {
  flex: 1 1 auto;
  display: flex;
  gap: 17px;
}

.link-action {
  font-size: 14px;
  font-weight: 500;
  color: #6f6f6f;
  text-decoration: none;
  cursor: pointer;
}

.link-action:hover {
  color: #1d1d1f;
}

.install-btn,
.done-btn {
  flex: 0 0 auto;
  min-width: 164px;
  box-sizing: border-box;
  font-size: 14px;
  padding: 12px 23px 13px;
  border-radius: 9px;
  border: 1px solid #f7b800;
  background: #ffc20a;
  color: #1c1c1c;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
  box-shadow: 0 1px 1px rgba(120, 86, 0, 0.18);
}

.install-btn:hover {
  background: #f5b900;
}

.done-btn {
  margin-left: auto;
}
</style>
