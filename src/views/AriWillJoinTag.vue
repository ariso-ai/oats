<template>
  <span class="ari-tag" :class="{ 'ari-tag--joined': joined }" :title="title">
    <svg class="ari-tag__icon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M8 1.5 9.6 6 14 8 9.6 10 8 14.5 6.4 10 2 8l4.4-2L8 1.5Z" fill="currentColor" />
    </svg>
    <span>{{ label }}</span>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue';

// `joined` flips the chip to the present tense once Ari is actually in the
// meeting; callers decide that from the meeting's status via `ariJoinChip`.
const props = defineProps<{ joined?: boolean }>();

const label = computed(() => (props.joined ? 'Ari has joined' : 'Ari will join'));
const title = computed(() =>
  props.joined
    ? 'Ari (the notetaker) has joined this meeting'
    : 'Ari (the notetaker) is scheduled to join this meeting'
);
</script>

<style scoped>
.ari-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 999px;
  background: #eef2ff;
  color: #4f46e5;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}
/* Ari is in the meeting: green reads as "live/active" next to the indigo
   "scheduled" state, matching the canceled chip's colour-per-state pattern. */
.ari-tag--joined {
  background: #ecfdf5;
  color: #047857;
}
.ari-tag__icon {
  width: 12px;
  height: 12px;
}
</style>
