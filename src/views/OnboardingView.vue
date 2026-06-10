<template>
  <div class="onboarding">
    <template v-if="step === 'signin'">
      <img class="logo" src="../assets/ariso-logo.png" alt="Ariso" />
      <h1 class="heading">Welcome to Ariso</h1>
      <p class="subheading">Sign in to sync your meetings and notes across devices.</p>

      <button
        :disabled="isSigningIn"
        class="google-btn"
        @click="handleGoogleSignIn"
      >
        <svg class="google-icon" viewBox="0 0 24 24">
          <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
          <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
          <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
          <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
        </svg>
        <span v-if="!isSigningIn">Sign in with Google</span>
        <span v-else>Signing in…</span>
      </button>

      <p v-if="errorMessage" class="error">{{ errorMessage }}</p>

      <button
        class="skip-btn"
        :disabled="isSigningIn"
        @click="handleSkip"
      >
        Skip for now
      </button>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { emit } from '@tauri-apps/api/event';
import { AUTH_SIGNED_IN_EVENT, auth, openSettingsWindow, setOnboarded } from '../tauri';
import { emitNotificationsSync } from '../composables/useMeetingNotifications';
import { ONBOARDING_STEPS, nextStepIndex } from './onboarding';

const currentStep = ref(0);
const step = computed(() => ONBOARDING_STEPS[currentStep.value]);

const isSigningIn = ref(false);
const errorMessage = ref('');

// Persist the "finished onboarding" flag once, at the end of the flow, then
// close this window. Closing is harmless — onboarding is not a background worker.
async function finishOnboarding({ openSettings = false } = {}) {
  await setOnboarded(true);
  if (openSettings) {
    await openSettingsWindow();
  }
  try {
    await getCurrentWindow().close();
  } catch {
    /* window already gone — ignore */
  }
}

// Advance to the next step, or hand the user back to Settings when onboarding
// is done. Skip and successful sign-in should land in the same native UI.
async function advance() {
  const next = nextStepIndex(ONBOARDING_STEPS, currentStep.value);
  if (next === null) {
    try {
      await finishOnboarding({ openSettings: true });
    } catch (error) {
      errorMessage.value = error instanceof Error ? error.message : 'Could not finish onboarding';
    }
  } else {
    currentStep.value = next;
  }
}

async function handleGoogleSignIn() {
  isSigningIn.value = true;
  errorMessage.value = '';
  try {
    const result = await auth.googleSignIn();
    if (result.error) {
      // Treat a user-closed auth window as a silent cancel (matches Settings).
      if (result.error !== 'Auth window closed') {
        errorMessage.value = result.error;
      }
      return;
    }
    void emitNotificationsSync().catch((err) => {
      console.warn('Failed to sync notifications after sign-in', err);
    });
    void emit(AUTH_SIGNED_IN_EVENT).catch((err) => {
      console.warn('Failed to broadcast desktop sign-in', err);
    });
    await finishOnboarding({ openSettings: true });
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Sign in failed';
  } finally {
    isSigningIn.value = false;
  }
}

async function handleSkip() {
  if (isSigningIn.value) {
    return;
  }
  await advance();
}
</script>

<style scoped>
.onboarding {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 12px;
  padding: 32px 28px;
  min-height: 100vh;
  box-sizing: border-box;
  font-family: -apple-system, system-ui, sans-serif;
  background: #f5f5f7;
}

.logo {
  width: 64px;
  height: 64px;
  object-fit: contain;
  margin-bottom: 4px;
}

.heading {
  font-size: 22px;
  font-weight: 700;
  color: #1d1d1f;
  margin: 0;
}

.subheading {
  font-size: 14px;
  color: #6b7280;
  margin: 0 0 12px;
  max-width: 320px;
}

.google-btn {
  width: 100%;
  max-width: 320px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 10px 16px;
  background: white;
  border: 1px solid #d1d5db;
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  font-size: 14px;
  font-weight: 500;
  color: #374151;
  cursor: pointer;
  transition: background 0.15s;
}

.google-btn:hover {
  background: #f9fafb;
}

.google-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.google-icon {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
}

.error {
  font-size: 12px;
  color: #f87171;
  margin: 0;
}

.skip-btn {
  margin-top: 4px;
  font-size: 13px;
  font-weight: 500;
  color: #6b7280;
  background: none;
  border: none;
  cursor: pointer;
}

.skip-btn:hover {
  color: #1d1d1f;
  text-decoration: underline;
}

.skip-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  text-decoration: none;
}
</style>
