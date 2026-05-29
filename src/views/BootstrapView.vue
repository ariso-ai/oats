<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  startMeetingNotifications,
  stopMeetingNotifications,
  SYNC_EVENT,
} from '../composables/useMeetingNotifications';

let unlisten: UnlistenFn | null = null;

async function sync() {
  await stopMeetingNotifications();
  await startMeetingNotifications();
}

onMounted(async () => {
  await startMeetingNotifications();
  unlisten = await listen(SYNC_EVENT, () => {
    void sync();
  });
});

onUnmounted(() => {
  unlisten?.();
  void stopMeetingNotifications();
});
</script>

<template>
  <div style="display: none" />
</template>
