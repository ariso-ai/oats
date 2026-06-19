<!-- src/views/MeetingPromptView.vue -->
<script setup lang="ts">
import { ref } from 'vue';
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

// Whether the "more options" menu below the Take notes button is open. Rust
// grows the (fixed-size, overflow-hidden) window so the menu has room to show.
const menuOpen = ref(false);

async function toggleMenu() {
  menuOpen.value = !menuOpen.value;
  try {
    await invoke('resize_meeting_prompt', { expanded: menuOpen.value });
  } catch {
    // No window in unit tests.
  }
}

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
  <div class="stage">
    <div class="prompt">
      <!-- macOS-style close button: pinned to the top-left corner, always shown -->
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

        <!-- Split button: Take notes + a chevron that reveals more options. -->
        <div class="split">
          <button class="primary-btn split-main" @click="resolve(true)">Take notes</button>
          <button
            data-test="more-options"
            class="primary-btn split-chevron"
            :class="{ 'split-chevron--open': menuOpen }"
            aria-label="More options"
            :aria-expanded="menuOpen"
            @click="toggleMenu"
          >
            <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
              <path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- More-options menu, anchored under the Take notes button. -->
    <div v-if="menuOpen" class="menu">
      <button data-test="menu-dismiss" class="menu-item" @click="resolve(false)">Dismiss</button>
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
/* Fills the window; holds the card and the menu that grows below it. */
.stage {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
}

.prompt {
  position: relative;
  flex: none;
  box-sizing: border-box;
  /* Matches MEETING_PROMPT_H (collapsed window height) so content stays centered
     with equal top/bottom padding even when the window grows for the menu. */
  height: 64px;
  display: flex;
  overflow: hidden;
  user-select: none;
  border-radius: 14px;
  background: #f7f6f4; /* Backdrop/Primary — matches the Meetings & Settings windows */
  color: #1c1c1c;
  border: 1px solid #e5e6e3;
}

/* macOS-style dismiss: pinned tight into the top-left corner, always visible. */
.dismiss {
  position: absolute;
  left: 4px;
  top: 4px;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.08);
  color: #6f6f6f;
  font-size: 10px;
  line-height: 1;
  cursor: pointer;
  transition: background 0.15s;
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

/* Split button — "Take notes" joined to a chevron, both mirror Settings'
   `.primary-btn`. */
.split {
  flex: none;
  display: flex;
  align-items: stretch;
}
.primary-btn {
  font-size: 13px;
  border: none;
  background: #1c1c1c;
  color: white;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
}
.split-main {
  padding: 6px 10px 6px 14px;
  border-radius: 999px 0 0 999px;
}
.split-chevron {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 10px 0 8px;
  border-radius: 0 999px 999px 0;
  border-left: 1px solid rgba(255, 255, 255, 0.18);
}
.split-chevron svg {
  transition: transform 0.15s;
}
.split-chevron--open svg {
  transform: rotate(180deg);
}

/* More-options menu, anchored directly under the split button (overlapping the
   empty bottom of the card so it reads as a dropdown attached to the button,
   not a panel floating far below it). */
.menu {
  position: absolute;
  top: 48px;
  right: 12px;
  min-width: 132px;
  padding: 5px;
  box-sizing: border-box;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 10px;
  box-shadow: 2px 2px 0 #e7e5e2;
}
.menu-item {
  width: 100%;
  text-align: left;
  font-size: 13px;
  font-weight: 500;
  font-family: inherit;
  color: #1c1c1c;
  background: none;
  border: none;
  border-radius: 7px;
  padding: 7px 10px;
  cursor: pointer;
}
.menu-item:hover {
  background: #f2f1ee;
}
</style>
