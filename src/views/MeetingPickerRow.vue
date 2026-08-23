<template>
  <button
    class="meeting-row"
    :class="{ 'meeting-row--featured': featured }"
    type="button"
    :disabled="disabled"
    @click="emit('choose')"
  >
    <span v-if="badge" class="meeting-badge">
      <span v-if="live" class="live-dot" aria-hidden="true" />
      {{ badge }}
    </span>
    <span class="meeting-title">{{ title || 'Untitled meeting' }}</span>
    <span class="meeting-time">{{ formattedTime }}</span>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue';

// One selectable meeting in the picker. `featured` is the picker's recommended
// choice (happening now / up next / the meeting the Library asked us to
// continue) and carries the primary, filled treatment — the plain rows are the
// quieter white cards around it.
const props = withDefaults(
  defineProps<{
    title: string;
    startAt: string;
    badge?: string | null;
    live?: boolean;
    featured?: boolean;
    disabled?: boolean;
  }>(),
  { badge: null, live: false, featured: false, disabled: false }
);

const emit = defineEmits<{ choose: [] }>();

const formattedTime = computed(() => {
  const d = new Date(props.startAt);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
});
</script>

<style scoped>
.meeting-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
  width: 100%;
  box-sizing: border-box;
  padding: 11px 13px;
  border: 1px solid #e7e5e2;
  border-radius: 12px;
  background: #ffffff;
  color: #1c1c1c;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s, background 0.12s, border-color 0.12s;
}

/* Rest is flat; hover lifts the card off the backdrop so the row reads as a
   target rather than a line of text. */
.meeting-row:not(:disabled):hover {
  border-color: #d6d6d6;
  box-shadow: 2px 2px 0 #e7e5e2;
  transform: translate(-1px, -1px);
}

.meeting-row:focus-visible {
  outline: 2px solid #1c1c1c;
  outline-offset: 2px;
}

.meeting-row:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* The recommended meeting takes the primary treatment (same idiom as
   .btn-primary): filled, with the hard shadow that presses in on hover. */
.meeting-row--featured {
  border-color: #1c1c1c;
  background: #1c1c1c;
  color: #f7f6f4;
  box-shadow: 2px 2px 0 #e7e5e2;
}

.meeting-row--featured:not(:disabled):hover {
  border-color: #1c1c1c;
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}

.meeting-row--featured:focus-visible {
  outline-color: #6f6f6f;
}

.meeting-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1.2px;
  color: #9a9a96;
}

.live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #d96a5a;
  animation: live-pulse 1.6s ease-in-out infinite;
}

.meeting-title {
  font-size: 15px;
  font-weight: 500;
  color: inherit;
  line-height: 1.25;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meeting-time {
  font-size: 12px;
  color: #6f6f6f;
}

.meeting-row--featured .meeting-badge,
.meeting-row--featured .meeting-time {
  color: rgba(247, 246, 244, 0.72);
}

@keyframes live-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}

@media (prefers-reduced-motion: reduce) {
  .live-dot { animation: none; }
  .meeting-row { transition: none; }
}
</style>
