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

<style scoped>
.prompt {
  position: relative;
  overflow: hidden;
  user-select: none;
  border-radius: 16px;
  background: #f7f6f4; /* Backdrop/Primary — matches the Meetings & Settings windows */
  color: #1c1c1c;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  border: 1px solid #e5e6e3;
  box-shadow:
    0 12px 32px rgba(0, 0, 0, 0.28),
    2px 2px 0 #e7e5e2;
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

.countdown-track {
  height: 3px;
  width: 100%;
  background: rgba(0, 0, 0, 0.08);
}
.countdown-fill {
  height: 100%;
  transform-origin: left;
  background: #1c1c1c; /* design-system accent — matches the primary button */
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

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
}

.logo {
  flex: none;
  width: 40px;
  height: 40px;
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
