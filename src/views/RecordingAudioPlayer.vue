<template>
  <audio
    v-if="blobUrl"
    class="audio-el"
    :src="blobUrl"
    controls
    autoplay
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
import { ref, onBeforeUnmount } from 'vue';
import { local } from '../tauri';

const props = defineProps<{ id: string; hasAudio: boolean }>();

const blobUrl = ref<string | null>(null);
const loading = ref(false);
const errored = ref(false);

async function onPlay() {
  if (!props.hasAudio || loading.value || blobUrl.value) return;
  loading.value = true;
  errored.value = false;
  try {
    const buf = await local.readRecordingAudio(props.id);
    const blob = new Blob([buf], { type: 'audio/mpeg' });
    blobUrl.value = URL.createObjectURL(blob);
  } catch (e) {
    console.error('Failed to load recording audio', e);
    errored.value = true;
  } finally {
    loading.value = false;
  }
}

onBeforeUnmount(() => {
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
}
.play-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.play-btn--error {
  color: #dc2626;
  border-color: #fca5a5;
}
.audio-el {
  height: 32px;
  max-width: 100%;
}
</style>
