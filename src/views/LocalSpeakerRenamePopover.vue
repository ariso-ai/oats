<template>
  <teleport to="body">
    <div class="lsp-overlay" @click="emit('close')" />
    <div class="lsp-pop" :style="panelStyle">
      <div class="lsp-head">
        <h3>Rename speakers</h3>
        <span class="lsp-count">{{ rename.speakers.value.length }} detected</span>
        <button class="lsp-close" type="button" aria-label="Close" @click="emit('close')">
          <svg viewBox="0 0 24 24" class="ic"><path d="M6 6l12 12M18 6L6 18" /></svg>
        </button>
      </div>

      <p v-if="rename.error.value" class="lsp-err" role="alert">⚠ {{ rename.error.value }}</p>

      <ul class="lsp-list">
        <li v-for="s in rename.speakers.value" :key="s.id" class="lsp-row">
          <!-- Inline rather than a nested popover: the panel is narrow and its
               list scrolls, so anything absolutely positioned would be clipped
               (same reasoning as SpeakerAssignPopover's assign field). -->
          <span class="lsp-name">{{ s.label }}</span>
          <input
            v-if="rename.editingId.value === s.id"
            ref="editInput"
            class="lsp-input"
            type="text"
            :value="rename.draft.value"
            :disabled="rename.saving.value"
            :aria-label="`Rename ${s.label}`"
            @input="rename.draft.value = ($event.target as HTMLInputElement).value"
            @keydown.enter.prevent="rename.commit()"
            @keydown.esc.stop.prevent="rename.cancelEdit()"
          />
          <button v-else class="lsp-btn lsp-edit" type="button" @click="startEdit(s.id)">Rename</button>
        </li>
      </ul>

      <!-- A rename re-renders the transcript but deliberately never rewrites
           already-generated note prose (a find/replace over LLM output is too
           collision-prone — see the spec's non-goals). Without this line the
           mismatch just looks like a bug: the transcript says "Priya" and the
           note still says "Speaker 2". Sits outside the scrolling list so it is
           always visible. -->
      <p class="lsp-foot">Existing AI Notes keep the old name until you regenerate them.</p>
    </div>
  </teleport>
</template>

<script setup lang="ts">
// Rename a diarized speaker on a *local* recording. Deliberately much smaller
// than SpeakerAssignPopover: offline there is no org directory and no voice
// matching, so there is nothing to search, play, or auto-match — just a label.
//
// Shaped like the panels it sits beside (fixed panel over a full-viewport
// click-catcher, teleported to <body> so the card's `overflow: hidden` can't
// clip it).
// The input deliberately carries no `maxlength`: HTML counts it in UTF-16 code
// units, while `commit()` (and Rust) count Unicode code points, so an
// astral-plane label would be cut off at half the real limit. Over-length input
// is caught by `commit()` and reported in the panel's error line instead.
import { computed, nextTick, ref } from 'vue';
import type { LocalSpeakerRename } from '../composables/useLocalSpeakerRename';

interface AnchorRect { bottom: number; left: number }

const props = defineProps<{ rename: LocalSpeakerRename; anchor: AnchorRect | null }>();
const emit = defineEmits<{ close: [] }>();

const PANEL_WIDTH = 300;

const editInput = ref<HTMLInputElement[] | HTMLInputElement | null>(null);

const panelStyle = computed<Record<string, string>>(() => {
  const a = props.anchor;
  const width = `${PANEL_WIDTH}px`;
  if (!a) return { position: 'fixed', top: '120px', left: '24px', width };
  const left = Math.max(8, Math.min(a.left, window.innerWidth - PANEL_WIDTH - 8));
  return { position: 'fixed', top: `${a.bottom + 6}px`, left: `${left}px`, width };
});

function startEdit(speakerId: number): void {
  props.rename.startEdit(speakerId);
  void nextTick(() => {
    const el = editInput.value;
    const input = Array.isArray(el) ? el[0] : el;
    input?.focus();
    input?.select();
  });
}
</script>

<style scoped>
.lsp-overlay { position: fixed; inset: 0; z-index: 60; }
.lsp-pop {
  z-index: 61; background: #fff; border: 1px solid #e5e6e3; border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c; max-height: 70vh; display: flex; flex-direction: column; overflow: hidden;
}
.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }

.lsp-head { display: flex; align-items: center; gap: 8px; padding: 12px 14px; border-bottom: 1px solid #e5e6e3; }
.lsp-head h3 { margin: 0; font-size: 15px; font-weight: 600; }
.lsp-count { font-size: 11px; color: #9b9b9b; flex: 1; }
.lsp-close { background: none; border: none; padding: 2px; color: #6f6f6f; cursor: pointer; display: flex; }
.lsp-close:hover { color: #1c1c1c; }

.lsp-err { margin: 0; padding: 8px 14px; background: #fef2f2; border-bottom: 1px solid #fecaca; font-size: 12px; color: #b91c1c; }

.lsp-list { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; min-height: 0; }
.lsp-row { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid #f0eeed; }
.lsp-row:last-child { border-bottom: none; }
.lsp-name { flex: 1; min-width: 0; font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.lsp-btn {
  padding: 3px 9px; border-radius: 999px; border: none; background: #f0eeed; color: #535353;
  font-family: inherit; font-size: 11px; font-weight: 600; cursor: pointer; flex-shrink: 0;
}
.lsp-btn:hover { background: #e5e3e0; }
.lsp-input { width: 140px; flex-shrink: 0; height: 32px; padding: 0 10px; border: 1px solid #d6d6d6; border-radius: 8px; font-family: inherit; font-size: 13px; }
.lsp-input:focus { outline: none; border-color: #6c63c0; }
.lsp-input:disabled { background: #f7f6f4; color: #9b9b9b; }

.lsp-foot { margin: 0; padding: 9px 14px; border-top: 1px solid #e5e6e3; background: #faf9f7; font-size: 11px; line-height: 1.4; color: #6f6f6f; flex-shrink: 0; }
</style>
