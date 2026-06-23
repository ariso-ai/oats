<template>
  <span class="ari-tag" title="Ari (the notetaker) is scheduled to join this meeting">
    <svg class="ari-tag__icon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M8 1.5 9.6 6 14 8 9.6 10 8 14.5 6.4 10 2 8l4.4-2L8 1.5Z" fill="currentColor" />
    </svg>
    <span>Ari will join</span>
  </span>
</template>

<script setup lang="ts"></script>

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
.ari-tag__icon {
  width: 12px;
  height: 12px;
}
</style>
