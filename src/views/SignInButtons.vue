<template>
  <div class="sign-in-buttons">
    <button
      ref="googleBtn"
      type="button"
      :disabled="pending"
      class="google-btn"
      @click="emit('sign-in', 'google')"
    >
      <svg class="google-icon" viewBox="0 0 24 24" aria-hidden="true">
        <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
        <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
        <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
        <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
      </svg>
      <span v-if="signingInWith === 'google'">Continue in your browser…</span>
      <span v-else>Sign in with Google</span>
    </button>
    <button
      type="button"
      :disabled="pending"
      class="microsoft-btn"
      @click="emit('sign-in', 'microsoft')"
    >
      <svg class="microsoft-icon" viewBox="0 0 21 21" aria-hidden="true">
        <path fill="#F25022" d="M0 0h10v10H0z" />
        <path fill="#7FBA00" d="M11 0h10v10H11z" />
        <path fill="#00A4EF" d="M0 11h10v10H0z" />
        <path fill="#FFB900" d="M11 11h10v10H11z" />
      </svg>
      <span v-if="signingInWith === 'microsoft'">Continue in your browser…</span>
      <span v-else>Sign in with Microsoft</span>
    </button>
    <button v-if="pending" type="button" class="sign-in-cancel" @click="emit('cancel')">
      Cancel
    </button>
    <p v-if="errorMessage" class="sign-in-error" role="alert">{{ errorMessage }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import type { SignInProvider } from '../composables/useAccountState';

// The Google/Microsoft sign-in buttons, with the pending state, Cancel, and the
// inline error. Shared by the Settings Account card and the Meetings window's
// sign-in popover so the two can't drift apart.
const props = defineProps<{
  signingInWith: SignInProvider | null;
  errorMessage?: string;
}>();
const emit = defineEmits<{
  'sign-in': [provider: SignInProvider];
  cancel: [];
}>();

const pending = computed(() => props.signingInWith !== null);

const googleBtn = ref<HTMLButtonElement | null>(null);
defineExpose({
  /** Move focus to the first provider button, e.g. when a popover opens. */
  focus: () => googleBtn.value?.focus(),
});
</script>

<style scoped>
.sign-in-buttons {
  text-align: center;
}

.google-btn,
.microsoft-btn {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 10px 16px;
  background: #ffffff;
  border: 1px solid #d6d6d6;
  border-radius: 999px;
  box-shadow: 2px 2px 0 #e7e5e2;
  font-family: inherit;
  font-size: 14px;
  font-weight: 500;
  color: #1c1c1c;
  white-space: nowrap;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}

.microsoft-btn {
  margin-top: 8px;
}

.google-btn:hover:not(:disabled),
.microsoft-btn:hover:not(:disabled) {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}

.google-btn:disabled,
.microsoft-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.google-icon,
.microsoft-icon {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
}

.sign-in-cancel {
  margin-top: 8px;
  font-family: inherit;
  font-size: 13px;
  color: #f87171;
  background: none;
  border: none;
  cursor: pointer;
  font-weight: 500;
}

.sign-in-cancel:hover {
  text-decoration: underline;
}

.sign-in-error {
  margin: 8px 0 0;
  font-size: 12px;
  color: #f87171;
}
</style>
