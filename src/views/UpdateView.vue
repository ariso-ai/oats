<template>
  <div class="update-window">
    <header class="update-hero">
      <img class="app-icon" src="../assets/oats-light.png" alt="Ariso" />

      <div class="hero-copy">
        <p class="eyebrow">Ariso for Mac</p>
        <h1 class="title">Update Available</h1>
        <div class="subtitle">
          <span class="version-chip">{{ info.version }}</span>
          <span class="dot">·</span>
          <span>You have {{ currentVersion }}</span>
        </div>
      </div>
    </header>

    <div class="notes-card">
      <div class="notes-title">What's new</div>
      <div class="notes-body" v-html="renderedNotes"></div>
    </div>

    <div v-if="downloadState === 'idle' && downloadError" class="error-line">
      {{ downloadError }}
    </div>

    <div v-if="downloadState === 'downloading'" class="progress-row">
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: progressPct + '%' }"></div>
      </div>
      <div class="progress-label">{{ progressPct }}%</div>
    </div>

    <div v-if="downloadState === 'idle'" class="actions">
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
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { updater, type UpdateInfo } from '../tauri';

const currentVersion = __APP_VERSION__;

const info = ref<UpdateInfo>({ version: '', notes: '', mandatory: false });
const downloadState = ref<'idle' | 'downloading'>('idle');
const downloadError = ref('');
const downloaded = ref(0);
const total = ref<number | null>(null);

const progressPct = computed(() => {
  if (!total.value || total.value === 0) return 0;
  return Math.min(100, Math.floor((downloaded.value / total.value) * 100));
});

// Render Markdown-ish release notes for the compact updater window. GitHub
// release bodies are bullet-list-heavy, so this keeps the window fast and
// predictable without bringing a full Markdown parser into the desktop shell.
const renderedNotes = computed(() => {
  const text = info.value.notes || 'No release notes provided.';
  return text
    .split('\n')
    .map((line) => {
      const trimmed = line.trim();
      if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
        return `<div class="bullet">• ${escapeHtml(trimmed.slice(2))}</div>`;
      }
      if (trimmed.startsWith('### ')) {
        return `<div class="h3">${escapeHtml(trimmed.slice(4))}</div>`;
      }
      if (trimmed.startsWith('## ')) {
        return `<div class="h2">${escapeHtml(trimmed.slice(3))}</div>`;
      }
      if (trimmed === '') return '<br/>';
      return `<div>${escapeHtml(trimmed)}</div>`;
    })
    .join('');
});

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

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
    downloadError.value = e instanceof Error ? e.message : String(e);
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
</script>

<style scoped>
.update-window {
  background: #f5f5f7;
  padding: 24px 28px 20px;
  box-sizing: border-box;
  width: 100vw;
  font-family: Polymath, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  min-height: 100vh;
  max-height: 100vh;
  display: flex;
  flex-direction: column;
  color: #1d1d1f;
  overflow: hidden;
}

.update-hero {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 14px;
  margin: 6px 0 18px;
}

.app-icon {
  width: 68px;
  height: 68px;
  object-fit: contain;
  flex: 0 0 auto;
}

.hero-copy {
  min-width: 0;
}

.eyebrow {
  margin: 0 0 2px;
  font-size: 12px;
  font-weight: 600;
  color: #6b7280;
}

.title {
  font-size: 24px;
  line-height: 1.08;
  font-weight: 700;
  color: #202124;
  margin: 0 0 7px 0;
  letter-spacing: 0;
}

.subtitle {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
  color: #6f7785;
}

.version-chip {
  background: #ffcb14;
  color: #111113;
  padding: 2px 8px 3px;
  border-radius: 999px;
  font-weight: 600;
  line-height: 1;
}

.dot {
  color: #9ca3af;
}

.notes-card {
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid #e2e5ea;
  border-radius: 8px;
  padding: 13px 15px;
  width: min(100%, calc(100vw - 56px));
  max-width: 364px;
  box-sizing: border-box;
  align-self: flex-start;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  font-size: 13px;
  line-height: 1.48;
  color: #2d3138;
  box-shadow: 0 1px 2px rgba(17, 24, 39, 0.05);
  scrollbar-width: thin;
  scrollbar-color: #c9ced8 transparent;
}

.notes-title {
  position: sticky;
  top: -13px;
  z-index: 1;
  margin: -13px -15px 10px;
  padding: 11px 15px 8px;
  background: rgba(255, 255, 255, 0.96);
  border-bottom: 1px solid #eceff3;
  font-weight: 700;
  font-size: 13px;
  color: #202124;
}

.notes-body {
  overflow-wrap: anywhere;
}

.notes-body .bullet { padding-left: 2px; }
.notes-body .h2    { font-weight: 700; margin: 8px 0 3px; color: #202124; }
.notes-body .h3    { font-weight: 600; margin: 6px 0 2px; color: #202124; }

.error-line {
  margin-top: 9px;
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
  margin-top: 14px;
  width: min(100%, calc(100vw - 56px));
  max-width: 364px;
  box-sizing: border-box;
  align-self: flex-start;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  flex: 0 0 auto;
}

.left-actions {
  flex: 1 1 auto;
  display: flex;
  gap: 16px;
}

.link-action {
  font-size: 13px;
  font-weight: 600;
  color: #737987;
  text-decoration: none;
  cursor: pointer;
}

.link-action:hover {
  color: #1d1d1f;
}

.install-btn {
  flex: 0 0 auto;
  width: 128px;
  box-sizing: border-box;
  font-size: 13px;
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid #111113;
  background: #111113;
  color: #fffbeb;
  font-weight: 700;
  white-space: nowrap;
  cursor: pointer;
  box-shadow: 0 1px 1px rgba(17, 24, 39, 0.16);
}

.install-btn:hover {
  background: #2b2d31;
}
</style>
