<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { SYNC_EVENT } from '../composables/useMeetingNotifications';
import { AUTO_RECORD_SYNC_EVENT } from '../composables/useAutoRecord';

// The notification orchestrator lives natively in Rust; this window just asks
// it to re-evaluate (session + enabled toggle) on launch and whenever a sync
// is broadcast (sign-in/out, settings toggle).
let unlisten: UnlistenFn | null = null;
let unlistenAuto: UnlistenFn | null = null;
let disposed = false;

onMounted(async () => {
  // Register the listener first so a failing initial sync doesn't prevent
  // future sign-in/out and settings-toggle broadcasts from being seen.
  const off = await listen(SYNC_EVENT, () => {
    void invoke('sync_meeting_notifications');
  });
  // onUnmounted can fire before this await resolves; if it did, detach now
  // so the listener doesn't outlive the component.
  if (disposed) {
    off();
    return;
  }
  unlisten = off;
  void invoke('sync_meeting_notifications');

  const offAuto = await listen(AUTO_RECORD_SYNC_EVENT, () => {
    void invoke('sync_auto_record');
  });
  if (disposed) {
    offAuto();
  } else {
    unlistenAuto = offAuto;
    void invoke('sync_auto_record');
  }
});

onUnmounted(() => {
  disposed = true;
  unlisten?.();
  unlisten = null;
  unlistenAuto?.();
  unlistenAuto = null;
});
</script>

<template>
  <div style="display: none" />
</template>
