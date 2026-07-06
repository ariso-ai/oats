<template>
  <div
    v-if="open"
    class="rec-choice"
    role="dialog"
    aria-modal="true"
    aria-labelledby="rec-choice-title"
    @click.self="$emit('cancel')"
  >
    <div class="rec-choice__card">
      <h2 id="rec-choice-title" class="rec-choice__title">Start recording</h2>
      <p class="rec-choice__body">
        Continue recording "{{ meetingTitle || 'this meeting' }}", or start a new recording?
      </p>
      <div class="rec-choice__actions">
        <button class="secondary-btn rec-choice__cancel" @click="$emit('cancel')">Cancel</button>
        <button class="secondary-btn rec-choice__new" @click="$emit('new')">New recording</button>
        <button class="primary-btn rec-choice__continue" @click="$emit('continue')">
          Continue meeting
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ open: boolean; meetingTitle: string }>();
defineEmits<{ (e: 'continue'): void; (e: 'new'): void; (e: 'cancel'): void }>();
</script>

<style scoped>
.rec-choice {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.35);
  padding: 24px;
}
.rec-choice__card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  padding: 20px;
  max-width: 380px;
  box-shadow: 2px 2px 0 #e7e5e2;
}
.rec-choice__title {
  font-size: 16px;
  font-weight: 700;
  margin: 0 0 8px;
  color: #1c1c1c;
}
.rec-choice__body {
  font-size: 13px;
  color: #6f6f6f;
  margin: 0 0 16px;
  line-height: 1.5;
}
.rec-choice__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
