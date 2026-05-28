<template>
  <div class="update-window">
    <img class="app-icon" src="../assets/ariso-logo-w.png" alt="" />

    <h1 class="title">Update Available</h1>

    <div class="subtitle">
      <span class="version-chip">{{ info.version }}</span>
      <span class="dot">·</span>
      <span>You have {{ currentVersion }}</span>
    </div>

    <div class="notes-card">
      <div class="notes-title">What's New</div>
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

// Render Markdown-ish release notes. We deliberately do not pull in a
// full Markdown parser — GitHub release bodies are bullet-list-heavy
// and a tiny renderer covers 99% of cases without the dependency cost.
const renderedNotes = computed(() => {
  const text = info.value.notes || '_No release notes provided._';
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
  background: white;
  padding: 22px 22px 18px;
  font-family: -apple-system, system-ui, sans-serif;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.app-icon {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  align-self: center;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.25);
  margin-bottom: 12px;
}

.title {
  font-size: 16px;
  font-weight: 700;
  color: #1d1d1f;
  text-align: center;
  margin: 0 0 4px 0;
}

.subtitle {
  font-size: 12px;
  color: #86868b;
  text-align: center;
  margin-bottom: 16px;
}

.version-chip {
  background: #eef2ff;
  color: #4f46e5;
  padding: 1px 7px;
  border-radius: 4px;
  font-weight: 600;
}

.dot {
  margin: 0 5px;
}

.notes-card {
  background: #f5f5f7;
  border-radius: 8px;
  padding: 12px 14px;
  flex: 1;
  overflow-y: auto;
  font-size: 12px;
  line-height: 1.6;
  color: #1d1d1f;
  max-height: 140px;
}

.notes-title {
  font-weight: 600;
  margin-bottom: 6px;
  font-size: 12px;
}

.notes-body .bullet { padding-left: 4px; }
.notes-body .h2    { font-weight: 700; margin: 6px 0 2px; }
.notes-body .h3    { font-weight: 600; margin: 4px 0 2px; }

.error-line {
  margin-top: 10px;
  font-size: 12px;
  color: #dc2626;
  text-align: center;
}

.progress-row {
  margin-top: 16px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.progress-track {
  flex: 1;
  height: 6px;
  background: #e5e7eb;
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(to right, #6366f1, #4f46e5);
  transition: width 0.2s;
}

.progress-label {
  font-size: 11px;
  color: #6b7280;
  width: 32px;
  text-align: right;
}

.actions {
  margin-top: 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.left-actions {
  display: flex;
  gap: 14px;
}

.link-action {
  font-size: 12px;
  color: #86868b;
  text-decoration: none;
  cursor: pointer;
}

.link-action:hover {
  color: #1d1d1f;
}

.install-btn {
  font-size: 13px;
  padding: 6px 18px;
  border-radius: 6px;
  border: none;
  background: linear-gradient(to bottom, #6366f1, #4f46e5);
  color: white;
  font-weight: 600;
  cursor: pointer;
}

.install-btn:hover {
  filter: brightness(1.05);
}
</style>
