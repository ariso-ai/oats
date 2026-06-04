<template>
  <audio
    v-if="blobUrl"
    ref="audioEl"
    class="audio-el"
    :src="blobUrl"
    controls
  ></audio>
  <button
    v-else
    class="play-btn"
    :class="{ 'play-btn--error': errored }"
    :disabled="!hasAudio || loading"
    @click="onPlay"
  >
    <span v-if="!hasAudio">▶ No audio</span>
    <span v-else-if="loading">Loading…</span>
    <span v-else-if="errored">Failed</span>
    <span v-else>▶ Play</span>
  </button>
</template>

<script setup lang="ts">
import { ref, onBeforeUnmount, nextTick } from 'vue';
import { local } from '../tauri';

const props = defineProps<{ id: string; hasAudio: boolean }>();

const blobUrl = ref<string | null>(null);
const loading = ref(false);
const errored = ref(false);
const audioEl = ref<HTMLAudioElement | null>(null);

// Tracks whether the component unmounted while a load was in flight, so a URL
// created after onBeforeUnmount ran gets revoked instead of leaking.
let destroyed = false;

async function onPlay() {
  if (!props.hasAudio || loading.value || blobUrl.value) return;
  loading.value = true;
  errored.value = false;
  try {
    const buf = await local.readRecordingAudio(props.id);
    // Backend writes recording.mp3 (see src-tauri/src/commands.rs); keep MIME in sync.
    const blob = new Blob([buf], { type: 'audio/mpeg' });
    const url = URL.createObjectURL(blob);
    if (destroyed) {
      URL.revokeObjectURL(url);
      return;
    }
    blobUrl.value = url;
    // autoplay is unreliable in WKWebView (the user-gesture token is lost across
    // the await), so play programmatically as a best effort; the native
    // <audio controls> remains usable if the webview blocks it.
    await nextTick();
    audioEl.value?.play().catch(() => {});
  } catch (e) {
    if (!destroyed) {
      console.error('Failed to load recording audio', e);
      errored.value = true;
    }
  } finally {
    if (!destroyed) {
      loading.value = false;
    }
  }
}

onBeforeUnmount(() => {
  destroyed = true;
  if (blobUrl.value) {
    URL.revokeObjectURL(blobUrl.value);
  }
});
</script>

<style scoped>
.play-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: 1px solid #d1d5db;
  background: white;
  color: #1d1d1f;
  cursor: pointer;
  flex-shrink: 0;
  white-space: nowrap;
}
.play-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.play-btn--error {
  color: #dc2626;
  border-color: #fca5a5;
}
/* Flex to fill the remaining row width beside the Note/Transcript buttons;
   min-width: 0 lets it shrink instead of pushing the buttons off the line. */
.audio-el {
  height: 32px;
  flex: 1 1 auto;
  min-width: 0;
}
</style>
