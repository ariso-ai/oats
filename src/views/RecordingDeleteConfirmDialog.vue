<template>
  <div
    v-if="open"
    class="rec-del"
    role="dialog"
    aria-modal="true"
    aria-labelledby="rec-del-title"
    @click.self="$emit('cancel')"
  >
    <div class="rec-del__card">
      <h2 id="rec-del-title" class="rec-del__title">Delete this recording?</h2>
      <p class="rec-del__body">
        This removes the selected recording and its transcript from the meeting.
        The meeting's other recordings and notes are kept. This can't be undone.
      </p>
      <div class="rec-del__actions">
        <button class="secondary-btn" @click="$emit('cancel')">Cancel</button>
        <button class="danger-btn" @click="$emit('confirm')">Delete</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ open: boolean }>();
defineEmits<{ (e: 'confirm'): void; (e: 'cancel'): void }>();
</script>

<style scoped>
.rec-del {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.35);
  padding: 24px;
}
.rec-del__card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  padding: 20px;
  max-width: 360px;
  box-shadow: 2px 2px 0 #e7e5e2;
}
.rec-del__title {
  font-size: 16px;
  font-weight: 700;
  margin: 0 0 8px;
  color: #1c1c1c;
}
.rec-del__body {
  font-size: 13px;
  color: #6f6f6f;
  margin: 0 0 16px;
  line-height: 1.5;
}
.rec-del__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.danger-btn {
  font-size: 13px;
  padding: 6px 14px;
  border-radius: 6px;
  border: 1px solid #fca5a5;
  background: #dc2626;
  color: #ffffff;
  cursor: pointer;
}
</style>
