<!-- src/views/MeetingPromptView.vue -->
<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { parsePromptParams } from './meetingPromptParams';
import oatsLogo from '../assets/oats-light.svg';

// Read params straight from the URL (no router dependency, so the view mounts
// bare in tests). The window is opened by Rust with `?seconds=<timeout>`.
const search = window.location.hash.includes('?')
  ? window.location.hash.slice(window.location.hash.indexOf('?'))
  : '';
const { seconds, title, subtitle } = parsePromptParams(search);

// Report the choice to Rust (which records / honors it), then close the banner
// right away for snappy feedback. Rust also tears the window down on decision or
// timeout, so this close is a harmless no-op if it gets there first.
async function resolve(record: boolean) {
  await invoke('resolve_meeting_prompt', { record });
  try {
    await getCurrentWebviewWindow().close();
  } catch {
    // No window in unit tests, or Rust already closed it.
  }
}
</script>

<template>
  <div
    class="group relative select-none overflow-hidden rounded-2xl bg-neutral-200/95 text-neutral-900 shadow-2xl backdrop-blur dark:bg-neutral-800/95 dark:text-white"
  >
    <!-- macOS-style close button: top-left corner, revealed on hover -->
    <button
      data-test="dismiss"
      class="absolute left-1.5 top-1.5 z-10 flex h-5 w-5 cursor-pointer items-center justify-center rounded-full bg-black/20 text-[11px] leading-none text-neutral-700 opacity-0 transition-opacity hover:bg-black/30 group-hover:opacity-100 dark:bg-white/25 dark:text-white"
      aria-label="Dismiss"
      @click="resolve(false)"
    >
      ✕
    </button>

    <!-- countdown bar: cosmetic, synced to the Rust timeout -->
    <div class="h-1 w-full bg-black/10 dark:bg-white/15">
      <div
        data-test="countdown-fill"
        class="h-full origin-left bg-[#0a84ff] countdown-fill"
        :style="{ animationDuration: `${seconds}s` }"
      ></div>
    </div>

    <div class="flex items-center gap-3 px-3 py-3">
      <img :src="oatsLogo" alt="oats" class="h-10 w-10 flex-none object-contain" />
      <div class="min-w-0 flex-1">
        <div class="text-sm font-semibold">{{ title }}</div>
        <div class="truncate text-[13px] text-neutral-500 dark:text-white/60">{{ subtitle }}</div>
      </div>

      <button
        class="flex-none cursor-pointer rounded-lg bg-[#0a84ff] px-3.5 py-1.5 text-[13px] font-semibold text-white"
        @click="resolve(true)"
      >
        Take notes
      </button>
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
