<template>
  <div
    v-if="open"
    class="ari-confirm"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ari-confirm-title"
    @click.self="$emit('cancel')"
  >
    <div class="ari-confirm__card">
      <h2 id="ari-confirm-title" class="ari-confirm__title">Ari is joining this meeting</h2>
      <p class="ari-confirm__body">
        Ari is scheduled to join this meeting and take notes. Do you still want to record?
      </p>
      <div class="ari-confirm__actions">
        <button class="secondary-btn" @click="$emit('cancel')">Cancel</button>
        <button class="primary-btn" @click="$emit('confirm')">Record anyway</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ open: boolean }>();
defineEmits<{ (e: 'confirm'): void; (e: 'cancel'): void }>();
</script>

<style scoped>
.ari-confirm {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.35);
  padding: 24px;
}
.ari-confirm__card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  padding: 20px;
  max-width: 360px;
  box-shadow: 2px 2px 0 #e7e5e2;
}
.ari-confirm__title {
  font-size: 16px;
  font-weight: 700;
  margin: 0 0 8px;
  color: #1c1c1c;
}
.ari-confirm__body {
  font-size: 13px;
  color: #6f6f6f;
  margin: 0 0 16px;
  line-height: 1.5;
}
.ari-confirm__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
