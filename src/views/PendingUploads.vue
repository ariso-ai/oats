<template>
  <div v-if="items.length > 0" class="pending">
    <div class="group-label">Pending uploads</div>
    <div v-for="it in items" :key="it.createdAt" class="pending-item">
      <span class="pi-title">{{ titleFor(it) }}</span>
      <span class="pi-dur">{{ durationFor(it) }}</span>
    </div>
    <p v-if="error" class="pending-error">Upload failed — try again.</p>
    <div class="pending-actions">
      <button class="pending-btn upload" :disabled="busy" @click="onUpload">
        <span v-if="busy" class="spinner" />
        <span v-else>Upload ({{ items.length }})</span>
      </button>
      <button class="pending-btn discard" :disabled="busy" @click="onDiscard">
        {{ confirmingDiscard ? 'Confirm discard' : 'Discard all' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { pending, type PendingUploadMeta } from '../tauri';
import { combineAndUpload, discardAll } from '../composables/usePendingUploads';

const emit = defineEmits<{ uploaded: [] }>();

const items = ref<PendingUploadMeta[]>([]);
const busy = ref(false);
const error = ref(false);
const confirmingDiscard = ref(false);

async function refresh(): Promise<void> {
  try {
    items.value = await pending.list();
  } catch (e) {
    console.error('Failed to list pending uploads', e);
    items.value = [];
  }
}
defineExpose({ refresh });

function titleFor(it: PendingUploadMeta): string {
  const d = new Date(it.startAt ?? it.createdAt);
  if (Number.isNaN(d.getTime())) return 'Recording';
  return `Recording ${d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}`;
}

function durationFor(it: PendingUploadMeta): string {
  const mins = Math.floor(it.durationSeconds / 60).toString().padStart(2, '0');
  const secs = (it.durationSeconds % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
}

async function onUpload(): Promise<void> {
  busy.value = true;
  error.value = false;
  confirmingDiscard.value = false;
  try {
    await combineAndUpload(items.value);
    await refresh();
    emit('uploaded');
  } catch (e) {
    console.error('Pending upload failed', e);
    error.value = true;
  } finally {
    busy.value = false;
  }
}

async function onDiscard(): Promise<void> {
  if (!confirmingDiscard.value) {
    confirmingDiscard.value = true;
    return;
  }
  busy.value = true;
  try {
    await discardAll(items.value);
    await refresh();
  } catch (e) {
    console.error('Discard all failed', e);
  } finally {
    confirmingDiscard.value = false;
    busy.value = false;
  }
}

onMounted(refresh);
</script>

<style scoped>
.pending {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 12px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.group-label {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: #f9a8a8;
}
.pending-item {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: #e5e5e5;
}
.pi-dur { color: #9ca3af; font-variant-numeric: tabular-nums; }
.pending-error { font-size: 11px; color: #f87171; margin: 0; }
.pending-actions { display: flex; gap: 8px; margin-top: 2px; }
.pending-btn {
  flex: 1;
  height: 28px;
  border: none;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.pending-btn:disabled { opacity: 0.6; cursor: default; }
.upload { background: #f9d852; color: #0d0d0d; }
.discard { background: #1f1f1f; color: #f87171; }
.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #4b5563;
  border-top-color: #0d0d0d;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
