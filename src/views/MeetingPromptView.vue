<!-- src/views/MeetingPromptView.vue -->
<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { parsePromptParams } from './meetingPromptParams';

// Read params straight from the URL (no router dependency, so the view mounts
// bare in tests). The window is opened by Rust with `?seconds=<timeout>`.
const search = window.location.hash.includes('?')
  ? window.location.hash.slice(window.location.hash.indexOf('?'))
  : '';
const { seconds, title, subtitle } = parsePromptParams(search);

const menuOpen = ref(false);

// Rust owns the clock and closes the window after it receives the decision, so
// the view only reports the choice — it does not close itself.
async function resolve(record: boolean) {
  await invoke('resolve_meeting_prompt', { record });
}
</script>

<template>
  <div
    class="select-none overflow-hidden rounded-2xl bg-neutral-200/95 text-neutral-900 shadow-2xl backdrop-blur dark:bg-neutral-800/95 dark:text-white"
  >
    <!-- countdown bar: cosmetic, synced to the Rust timeout -->
    <div class="h-1 w-full bg-black/10 dark:bg-white/15">
      <div
        data-test="countdown-fill"
        class="h-full origin-left bg-[#0a84ff] countdown-fill"
        :style="{ animationDuration: `${seconds}s` }"
      ></div>
    </div>

    <div class="flex items-center gap-3 px-3 py-3">
      <div
        class="flex h-10 w-10 flex-none items-center justify-center rounded-lg bg-gradient-to-br from-[#f5c518] to-[#e69b00] text-xl"
      >
        🥣
      </div>
      <div class="min-w-0 flex-1">
        <div class="text-sm font-semibold">{{ title }}</div>
        <div class="truncate text-[13px] text-neutral-500 dark:text-white/60">{{ subtitle }}</div>
      </div>

      <div class="relative flex flex-none items-stretch">
        <button
          class="rounded-l-lg bg-[#0a84ff] px-3.5 py-1.5 text-[13px] font-semibold text-white"
          @click="resolve(true)"
        >
          Take notes
        </button>
        <button
          data-test="disclosure"
          class="rounded-r-lg border-l border-white/20 bg-[#0a84ff] px-2 py-1.5 text-[13px] text-white"
          aria-label="More actions"
          @click="menuOpen = !menuOpen"
        >
          ⌄
        </button>
        <div
          v-if="menuOpen"
          class="absolute right-0 top-full z-10 mt-1 w-32 overflow-hidden rounded-lg bg-white shadow-xl dark:bg-neutral-700"
        >
          <button
            class="block w-full px-3 py-2 text-left text-[13px] text-neutral-900 hover:bg-black/5 dark:text-white dark:hover:bg-white/10"
            @click="resolve(false)"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.countdown-fill {
  animation-name: countdown-drain;
  animation-timing-function: linear;
  animation-fill-mode: forwards;
}
@keyframes countdown-drain {
  from {
    transform: scaleX(1);
  }
  to {
    transform: scaleX(0);
  }
}
</style>
