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
  <div class="prompt group">
    <!-- macOS-style close button: top-left corner, revealed on hover -->
    <button data-test="dismiss" class="dismiss" aria-label="Dismiss" @click="resolve(false)">✕</button>

    <!-- countdown bar: cosmetic, synced to the Rust timeout -->
    <div class="countdown-track">
      <div
        data-test="countdown-fill"
        class="countdown-fill"
        :style="{ animationDuration: `${seconds}s` }"
      ></div>
    </div>

    <div class="row">
      <img :src="oatsLogo" alt="oats" class="logo" />
      <div class="copy">
        <div class="title">{{ title }}</div>
        <div class="subtitle">{{ subtitle }}</div>
      </div>

      <button class="primary-btn" @click="resolve(true)">Take notes</button>
    </div>
  </div>
</template>

<style>
/* Global, intentionally NOT scoped: this view owns a borderless, transparent
   window (like the waveform pill). Kill the default body margin and any opaque
   background so the card fills the window edge-to-edge with no surrounding
   channel, and the rounded corners reveal the desktop instead of a frame. */
html,
body,
#app {
  margin: 0;
  padding: 0;
  background: transparent !important;
  height: 100%;
  overflow: hidden;
}
</style>

<style scoped>
.prompt {
  position: relative;
  box-sizing: border-box;
  width: 100vw;
  height: 100vh;
  display: flex;
  overflow: hidden;
  user-select: none;
  border-radius: 14px;
  background: #f7f6f4; /* Backdrop/Primary — matches the Meetings & Settings windows */
  color: #1c1c1c;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  border: 1px solid #e5e6e3;
}

/* macOS-style dismiss: top-left, revealed on hover. */
.dismiss {
  position: absolute;
  left: 8px;
  top: 8px;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.08);
  color: #6f6f6f;
  font-size: 11px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s, background 0.15s;
}
.group:hover .dismiss {
  opacity: 1;
}
.dismiss:hover {
  background: rgba(0, 0, 0, 0.14);
}

/* Overlay at the very top edge so it doesn't consume layout height — keeps the
   content row's top and bottom padding symmetric. */
.countdown-track {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 1;
  height: 3px;
  background: rgba(0, 0, 0, 0.08);
}
/* Animate `width` (not `transform`) so the bar is clipped correctly by the
   card's rounded corners — WebKit fails to clip a transformed child against a
   rounded `overflow: hidden` parent, which left a square nub in the corner. */
.countdown-fill {
  height: 100%;
  width: 100%;
  background: #1c1c1c; /* design-system accent — matches the primary button */
  animation-name: countdown-drain;
  animation-timing-function: linear;
  animation-fill-mode: forwards;
}
@keyframes countdown-drain {
  from {
    width: 100%;
  }
  to {
    width: 0%;
  }
}

.row {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 12px;
  /* Horizontal padding only; the row fills the card height and centers its
     content, so top/bottom spacing stays equal and follows the window height. */
  padding: 0 12px;
}

.logo {
  flex: none;
  width: 38px;
  height: 38px;
  object-fit: contain;
}

.copy {
  min-width: 0;
  flex: 1;
}

.title {
  font-size: 14px;
  font-weight: 600;
  color: #1c1c1c;
}

.subtitle {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: #6f6f6f;
}

/* Primary button — mirrors `.primary-btn` in the Settings window. */
.primary-btn {
  flex: none;
  font-size: 13px;
  padding: 6px 14px;
  border-radius: 999px;
  border: none;
  background: #1c1c1c;
  color: white;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
}
</style>
