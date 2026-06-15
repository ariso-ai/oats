<template>
  <div class="update-window">
    <header class="update-hero">
      <img class="app-icon" src="../assets/oats-light.svg" alt="Ariso" />

      <div class="hero-copy">
        <h1 class="title">What&rsquo;s new for Oats?</h1>
        <p class="subtitle">
          Version {{ displayVersion }} is ready. You have {{ currentVersion }}.
        </p>
      </div>
    </header>

    <main class="update-content">
      <section class="highlights-section" aria-labelledby="update-highlights">
        <h2 id="update-highlights" class="section-title">Highlights</h2>
        <ul class="highlights-list">
          <li v-for="item in highlights" :key="item">{{ item }}</li>
        </ul>
      </section>

      <section class="benefit-section" aria-label="Update benefit">
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
      <div class="left-actions">
        <a
          v-if="!info.mandatory"
          href="#"
          @click.prevent="onSkip"
          class="link-action"
        >Skip</a>
        <a
          v-if="!info.mandatory"
          href="#"
          @click.prevent="onLater"
          class="link-action"
        >Later</a>
      </div>
      <button class="install-btn" @click="onInstall">Install Update</button>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { MicrophoneIcon } from '@heroicons/vue/24/outline';
import { updater, type UpdateInfo } from '../tauri';

const currentVersion = __APP_VERSION__;

const info = ref<UpdateInfo>({
  version: '',
  notes: '',
  mandatory: false,
});
const downloadState = ref<'idle' | 'downloading'>('idle');
const downloadError = ref('');
const downloaded = ref(0);
const total = ref<number | null>(null);

const progressPct = computed(() => {
  if (!total.value || total.value === 0) return 0;
  return Math.min(100, Math.floor((downloaded.value / total.value) * 100));
});

const displayVersion = computed(() => info.value.version || '0.4.0');

// Pull the first few Markdown bullet lines into the large Highlights section.
// Release bodies vary, so the fallback keeps preview and empty-note states
// aligned with the product design instead of showing raw markdown.
const highlights = computed(() => {
  const items = info.value.notes
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.startsWith('- ') || line.startsWith('* '))
    .map((line) => line.slice(2).trim())
    .filter(Boolean)
    .slice(0, 3);

  return items.length > 0
    ? items
    : [
        'Faster meeting notes',
        'Improved local transcription',
        'Cleaner update flow',
      ];
});

let unlistenProgress: UnlistenFn | null = null;
let unlistenAvailable: UnlistenFn | null = null;

onMounted(async () => {
  // Initial state (covers the case where the window is opened from
  // Settings → "Show Details" after the event already fired).
  try {
    const snap = await updater.getState();
    if (snap.latest_known) {
      info.value = snap.latest_known;
    }
  } catch (e) {
    if (isBrowserPreviewError(e)) {
      info.value = {
        version: '0.4.0',
        notes: [
          '## Highlights',
          '- Faster meeting notes',
          '- Improved local transcription',
          '- Cleaner update flow',
        ].join('\n'),
        mandatory: false,
      };
    } else {
      downloadError.value = e instanceof Error ? e.message : String(e);
    }
  }

  // Stay in sync if a fresh check fires while we're open.
  unlistenAvailable = await listen<UpdateInfo>('update://available', (e) => {
    info.value = e.payload;
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
  try {
    await updater.skipVersion(info.value.version);
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

// Plain browser previews do not have Tauri IPC, but designers still need a
// faithful visual render. Only that missing-IPC case falls back to mock content.
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
  background: #fbfbfa;
  padding: 0;
  box-sizing: border-box;
  width: 100%;
  font-family: Polymath, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  min-height: 100vh;
  max-height: 100vh;
  display: flex;
  flex-direction: column;
  color: #09090b;
  overflow: hidden;
}

.update-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 54px 48px 28px;
  text-align: center;
  flex: 0 0 auto;
}

.app-icon {
  width: 232px;
  height: auto;
  object-fit: contain;
  flex: 0 0 auto;
}

.hero-copy {
  min-width: 0;
  margin-top: -10px;
}

.title {
  font-size: 48px;
  line-height: 1.06;
  font-weight: 700;
  color: #060607;
  margin: 0 0 15px 0;
  letter-spacing: 0;
}

.subtitle {
  margin: 0;
  font-size: 30px;
  line-height: 1.28;
  font-weight: 400;
  color: #5b606d;
}

.update-content {
  width: calc(100% - 96px);
  max-width: 572px;
  margin: 0 auto;
  border-top: 1px solid #d9d9d8;
  border-bottom: 1px solid #d9d9d8;
  flex: 1 1 auto;
  overflow: hidden;
}

.highlights-section {
  padding: 40px 10px 22px;
}

.section-title {
  margin: 0 0 28px;
  font-size: 32px;
  line-height: 1.1;
  font-weight: 700;
  color: #060607;
}

.highlights-list {
  display: grid;
  gap: 24px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 28px;
  line-height: 1.18;
  color: #0a0a0c;
}

.highlights-list li {
  display: grid;
  grid-template-columns: 14px 1fr;
  align-items: baseline;
  column-gap: 30px;
}

.highlights-list li::before {
  content: "";
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #ffc20a;
  transform: translateY(-3px);
}

.benefit-section {
  display: grid;
  grid-template-columns: 72px 1fr;
  gap: 24px;
  align-items: center;
  border-top: 1px solid #d9d9d8;
  padding: 42px 14px 28px;
}

.mic-icon {
  width: 66px;
  height: 66px;
  fill: none;
  stroke: #545966;
  stroke-width: 3.3;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.benefit-section p {
  margin: 0;
  color: #535966;
  font-size: 26px;
  line-height: 1.28;
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
  gap: 24px;
  flex: 0 0 auto;
  min-height: 110px;
  padding: 0 40px 0 46px;
  background: rgba(246, 246, 245, 0.92);
  border-top: 1px solid #d9d9d8;
}

.left-actions {
  flex: 1 1 auto;
  display: flex;
  gap: 16px;
}

.link-action {
  font-size: 27px;
  font-weight: 600;
  color: #555b68;
  text-decoration: none;
  cursor: pointer;
}

.link-action:hover {
  color: #1d1d1f;
}

.install-btn {
  flex: 0 0 auto;
  min-width: 214px;
  box-sizing: border-box;
  font-size: 25px;
  padding: 14px 30px 15px;
  border-radius: 10px;
  border: 1px solid #f7b800;
  background: #ffc20a;
  color: #080809;
  font-weight: 700;
  white-space: nowrap;
  cursor: pointer;
  box-shadow: 0 1px 1px rgba(120, 86, 0, 0.18);
}

.install-btn:hover {
  background: #f5b900;
}
</style>
