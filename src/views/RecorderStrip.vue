<template>
  <div v-if="state" class="strip">
    <template v-if="state.phase === 'success' || state.phase === 'failed'">
      <span class="status-icon" :class="state.phase === 'success' ? 'ok' : 'err'">
        {{ state.phase === 'success' ? '✓' : '✗' }}
      </span>
      <span class="status-label">
        {{ state.phase === 'success' ? 'Recording saved' : 'Upload failed' }}
      </span>
    </template>
    <template v-else-if="state.phase === 'uploading'">
      <span class="spinner" />
      <span class="status-label">Saving…</span>
    </template>
    <template v-else>
      <div class="bars">
        <div
          v-for="(level, i) in state.bars"
          :key="i"
          class="bar"
          :class="{ paused: state.isPaused }"
          :style="{ height: `${Math.max(12, Math.min(100, Math.sqrt(level) * 150))}%` }"
        />
      </div>
      <span class="timer">{{ formattedDuration }}</span>
      <div class="controls">
        <button
          class="ctrl-btn pause-btn"
          :aria-label="state.isPaused ? 'Resume recording' : 'Pause recording'"
          @click.stop.prevent="state.isPaused ? control('tray://resume-recording') : control('tray://pause-recording')"
        >
          <svg v-if="!state.isPaused" width="14" height="14" viewBox="0 0 14 14">
            <rect x="2" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
            <rect x="8.5" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
          </svg>
          <svg v-else width="14" height="14" viewBox="0 0 14 14">
            <path d="M3 1.8v10.4c0 .7.8 1.2 1.4.8l8.1-5.2c.6-.4.6-1.2 0-1.6L4.4 1c-.6-.4-1.4.1-1.4.8z" fill="currentColor" />
          </svg>
        </button>
        <button class="ctrl-btn stop-btn" aria-label="Stop recording" @click.stop.prevent="control('tray://stop-recording')">
          <svg width="12" height="12" viewBox="0 0 12 12">
            <rect x="1" y="1" width="10" height="10" rx="2" fill="currentColor" />
          </svg>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';

/** Mirror of the recording running in the (hidden) waveform window. This
 *  component renders `recorder://state` broadcasts and sends the same tray
 *  control events the tray menu uses — it owns no audio. */
interface RecorderState {
  bars: number[];
  durationSeconds: number;
  isPaused: boolean;
  phase: 'recording' | 'uploading' | 'success' | 'failed' | 'closed';
}

const state = ref<RecorderState | null>(null);
let unlistenState: UnlistenFn | null = null;

const formattedDuration = computed(() => {
  const s = state.value?.durationSeconds ?? 0;
  const mins = Math.floor(s / 60).toString().padStart(2, '0');
  const secs = (s % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
});

function control(event: string): void {
  emit(event).catch((e) => console.error('Recorder control failed', e));
}

onMounted(async () => {
  unlistenState = await listen<RecorderState>('recorder://state', (e) => {
    state.value = e.payload.phase === 'closed' ? null : e.payload;
  });
});

onUnmounted(() => {
  unlistenState?.();
});
</script>

<style scoped>
/* Horizontal sibling of the floating pill: same dark surface, yellow bars,
   and control colors, laid out as a slim bottom strip. */
.strip {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 14px;
  height: 44px;
  flex-shrink: 0;
  margin-top: 10px;
  padding: 0 18px;
  border-radius: 22px;
  background: #0d0d0d;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
}

.bars {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 18px;
}

.bar {
  width: 3px;
  border-radius: 2px;
  background: #f9d852;
  transition: height 75ms, background 150ms;
}

.bar.paused {
  background: #4b5563;
}

.timer {
  font-size: 12px;
  font-weight: 600;
  color: #e5e5e5;
  font-variant-numeric: tabular-nums;
}

.controls {
  display: flex;
  align-items: center;
  gap: 6px;
}

.ctrl-btn {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: none;
  background: #1e1e1e;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 0.15s;
}
.ctrl-btn:hover { background: #2a2a2a; }
.pause-btn { color: #ffffff; }
.stop-btn { color: #f87171; }

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #4b5563;
  border-top-color: #f9d852;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.status-label {
  font-size: 12px;
  font-weight: 600;
  color: #e5e5e5;
}
.status-icon { font-size: 14px; font-weight: 700; }
.status-icon.ok { color: #34d399; }
.status-icon.err { color: #f87171; }

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
