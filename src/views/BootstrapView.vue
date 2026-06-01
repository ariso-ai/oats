<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { SYNC_EVENT } from '../composables/useMeetingNotifications';

// The notification orchestrator lives natively in Rust; this window just asks
// it to re-evaluate (session + enabled toggle) on launch and whenever a sync
// is broadcast (sign-in/out, settings toggle).
let unlisten: UnlistenFn | null = null;

onMounted(async () => {
  await invoke('sync_meeting_notifications');
  unlisten = await listen(SYNC_EVENT, () => {
    void invoke('sync_meeting_notifications');
  });
});

onUnmounted(() => {
  unlisten?.();
});
</script>

<template>
  <div style="display: none" />
</template>
