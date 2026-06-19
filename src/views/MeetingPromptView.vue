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
// The split button, measured so the Dismiss button can match its width.
const splitEl = ref<HTMLElement | null>(null);
const menuWidth = ref('auto');

async function toggleMenu() {
  menuOpen.value = !menuOpen.value;
  if (menuOpen.value && splitEl.value) {
    menuWidth.value = `${splitEl.value.offsetWidth}px`;
  }
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
    <!-- close button straddling the card's rounded top-left corner; lives in the
         stage (not the card) so it can sit on top of / outside the corner -->
    <button data-test="dismiss" class="dismiss" aria-label="Dismiss" @click="resolve(false)">✕</button>

    <div class="prompt">
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
        <div ref="splitEl" class="split">
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

    <!-- Dismiss, revealed under the Take notes button by the chevron. Same
         shape/size as Take notes, secondary styling. -->
    <button
      v-if="menuOpen"
      data-test="menu-dismiss"
      class="secondary-btn"
      :style="{ width: menuWidth }"
      @click="resolve(false)"
    >
      Dismiss
    </button>
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
/* Fills the window. Transparent padding insets the floating card so the corner
   dismiss button can straddle its top-left corner without being clipped. */
.stage {
  position: relative;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  padding: 13px;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
}

.prompt {
  position: relative;
  flex: none;
  box-sizing: border-box;
  /* Matches MEETING_PROMPT_H minus the stage padding (top+bottom) so content
     stays centered even when the window grows for the menu. */
  height: 58px;
  display: flex;
  overflow: hidden;
  user-select: none;
  border-radius: 14px;
  background: #f7f6f4; /* Backdrop/Primary — matches the Meetings & Settings windows */
  color: #1c1c1c;
  border: 1px solid #e5e6e3;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.16);
}

/* Dismiss: a bordered white circle straddling the card's top-left corner, on
   top of it (lives in the stage so it isn't clipped by the card). Always shown. */
.dismiss {
  position: absolute;
  left: 1px;
  top: 1px;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid #e0e0dd;
  border-radius: 999px;
  background: #ffffff;
  color: #6f6f6f;
  font-size: 12px;
  line-height: 1;
  cursor: pointer;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  transition: background 0.15s, color 0.15s;
}
.dismiss:hover {
  background: #f2f1ee;
  color: #1c1c1c;
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

/* Dismiss — a secondary button mirroring Settings' `.secondary-btn`, same
   pill shape/size as Take notes. Anchored directly under the split button
   (overlapping the empty bottom of the card so it reads as a dropdown). */
.secondary-btn {
  position: absolute;
  top: 56px;
  right: 25px;
  box-sizing: border-box;
  text-align: center;
  font-size: 13px;
  font-weight: 500;
  font-family: inherit;
  padding: 6px 14px;
  border-radius: 999px;
  border: 1px solid #d6d6d6;
  background: #ffffff;
  box-shadow: 2px 2px 0 #e7e5e2;
  color: #1c1c1c;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}
.secondary-btn:hover {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}
</style>
