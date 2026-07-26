<template>
  <span class="canceled-tag" title="This meeting was canceled">
    <svg class="canceled-tag__icon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.6" />
      <path d="M4.6 11.4 11.4 4.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    </svg>
    <span>Canceled</span>
  </span>
</template>

<script setup lang="ts"></script>

<style scoped>
.canceled-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 999px;
  background: #fef2f2;
  color: #b91c1c;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}
.canceled-tag__icon {
  width: 12px;
  height: 12px;
}
</style>
