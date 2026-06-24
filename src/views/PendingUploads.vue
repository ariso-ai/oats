<template>
  <div v-if="items.length > 0" class="pending">
    <div class="group-label">Pending uploads</div>
    <div class="pending-card">
      <div v-for="it in items" :key="it.createdAt" class="pending-item">
        <span class="pi-dot" aria-hidden="true" />
        <span class="pi-wave" aria-hidden="true" />
        <span class="pi-title">{{ titleFor(it) }}</span>
        <span class="pi-dur">{{ durationFor(it) }}</span>
      </div>
      <!-- Leaving the actions row cancels a pending discard confirmation so the
           destructive second click can't linger armed after the user moves away. -->
      <div class="pending-actions" @mouseleave="confirmingDiscard = false">
        <button class="pending-btn upload" :disabled="busy" @click="onUpload">
          <span v-if="busy" class="spinner" />
          <span v-else>Upload ({{ items.length }})</span>
        </button>
        <button class="pending-btn discard" :disabled="busy" @click="onDiscard">
          {{ confirmingDiscard ? 'Confirm discard' : 'Discard all' }}
        </button>
      </div>
    </div>
    <p v-if="error" class="pending-error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { auth, pending, type PendingUploadMeta } from '../tauri';
import { combineAndUpload, discardAll } from '../composables/usePendingUploads';

const emit = defineEmits<{ uploaded: [] }>();

const items = ref<PendingUploadMeta[]>([]);
const busy = ref(false);
const error = ref<string | null>(null);
const confirmingDiscard = ref(false);

// Keep pending-upload failures specific enough to explain auth/session states,
// while preserving a short generic fallback for transient network or S3 errors.
function uploadErrorMessage(err: unknown): string {
  const message = err instanceof Error ? err.message : String(err ?? '');
  if (/\b(401|403)\b|unauthori[sz]ed|forbidden|session|sign(?:ed)? in|login|auth/i.test(message)) {
    return 'Upload failed — sign in to Ari again, then retry.';
  }
  return 'Upload failed — try again.';
}

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

// Retry is only useful when desktop has an Ari session to attach to the upload
// request. Checking first lets the UI explain a signed-out state before the
// native API proxy collapses it into a generic network/backend failure.
async function onUpload(): Promise<void> {
  busy.value = true;
  error.value = null;
  confirmingDiscard.value = false;
  try {
    if (!(await auth.checkSession())) {
      error.value = 'Upload failed — sign in to Ari again, then retry.';
      return;
    }
    await combineAndUpload(items.value);
    await refresh();
    emit('uploaded');
  } catch (e) {
    console.error('Pending upload failed', e);
    error.value = uploadErrorMessage(e);
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
  padding: 6px;
  flex-shrink: 0;
}
.group-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1.5px;
  color: #9a9a96;
  padding: 14px 10px 4px;
}
.pending-card {
  width: 100%;
  box-sizing: border-box;
  overflow: hidden;
  border: 1px solid #dedbd6;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.72);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.65), 0 1px 2px rgba(28, 28, 28, 0.04);
}
.pending-item {
  display: flex;
  align-items: center;
  gap: 7px;
  min-height: 52px;
  padding: 0 10px;
  box-sizing: border-box;
}
.pending-item + .pending-item {
  border-top: 1px solid #e5e2dd;
}
.pi-dot {
  width: 12px;
  height: 12px;
  flex: 0 0 12px;
  border-radius: 999px;
  background: #a19d94;
  box-shadow: inset 0 1px 1px rgba(28, 28, 28, 0.18);
}
.pi-wave {
  width: 20px;
  height: 24px;
  flex: 0 0 20px;
  background:
    linear-gradient(#8f8c86, #8f8c86) 0 50% / 3px 10px no-repeat,
    linear-gradient(#8f8c86, #8f8c86) 6px 50% / 3px 24px no-repeat,
    linear-gradient(#8f8c86, #8f8c86) 12px 50% / 3px 17px no-repeat,
    linear-gradient(#8f8c86, #8f8c86) 18px 50% / 3px 10px no-repeat;
  border-radius: 999px;
}
.pi-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #1c1c1c;
  font-size: 14px;
  font-weight: 500;
  line-height: 1.25;
}
.pi-dur {
  flex-shrink: 0;
  color: #6f6f6f;
  font-size: 13px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.pending-error {
  margin: 0 10px;
  color: #c2413b;
  font-size: 12px;
}
.pending-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 14px;
  min-height: 56px;
  padding: 10px 12px;
  border-top: 1px solid #e5e2dd;
  box-sizing: border-box;
}
.pending-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 0;
  height: 34px;
  border: 1px solid transparent;
  border-radius: 999px;
  font-family: inherit;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s, color 0.12s;
}
.pending-btn:disabled { opacity: 0.6; cursor: default; }
.upload {
  flex: 0 1 128px;
  background: #1c1c1c;
  color: #ffffff;
}
.upload:not(:disabled):hover { background: #343434; }
.discard {
  flex: 0 1 126px;
  background: rgba(255, 255, 255, 0.62);
  border-color: #d7d6d2;
  color: #6f6f6f;
}
.discard:not(:disabled):hover {
  background: #ffffff;
  border-color: #bdbbb6;
  color: #1c1c1c;
}
.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #ffffff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
