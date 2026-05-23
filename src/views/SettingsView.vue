<template>
  <div class="settings">
    <h1 class="title">Settings</h1>

    <div v-if="signInPrompt && !isSignedIn" class="signin-banner">
      Please sign in to start recording.
    </div>

    <!-- Account Section -->
    <section class="section">
      <h2 class="section-title">Account</h2>
      <div class="card">
        <div v-if="isSignedIn" class="account-info">
          <div class="avatar">{{ initials }}</div>
          <div class="account-details">
            <span class="account-name">{{ displayName }}</span>
            <span class="account-email">{{ email }}</span>
          </div>
          <button class="sign-out-btn" @click="handleSignOut">Sign Out</button>
        </div>
        <div v-else class="sign-in-container">
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
            <span v-else>Signing in...</span>
          </button>
          <p v-if="errorMessage" class="error">{{ errorMessage }}</p>
        </div>
      </div>
    </section>

    <!-- Audio Section -->
    <section class="section">
      <h2 class="section-title">Audio</h2>
      <div class="card">
        <div class="setting-row">
          <span class="setting-label">Recording mode</span>
          <select v-model="recordingMode" class="setting-select">
            <option value="mic">Microphone only</option>
            <option value="mic_and_system">Mic + System Audio</option>
          </select>
        </div>
      </div>
    </section>

    <!-- About Section -->
    <section class="section">
      <h2 class="section-title">About</h2>
      <div class="card">
        <span class="about-text">Ariso v{{ appVersion }}</span>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { auth, api } from '../tauri';
import { load } from '@tauri-apps/plugin-store';

const isSignedIn = ref(false);
const isSigningIn = ref(false);
const errorMessage = ref('');
const displayName = ref('');
const email = ref('');
const recordingMode = ref<'mic' | 'mic_and_system'>('mic_and_system');
const signInPrompt = ref(false);
const appVersion = __APP_VERSION__;

const initials = computed(() => {
  const name = displayName.value || email.value || '?';
  return name.slice(0, 2).toUpperCase();
});

watch(recordingMode, async (newMode) => {
  const store = await load('settings.json', { autoSave: true });
  await store.set('recordingMode', newMode);
});

async function fetchUserProfile() {
  try {
    const res = await api.request('GET', '/auth/me');
    const data = res.data as { full_name?: string; email?: string };
    displayName.value = data.full_name || '';
    email.value = data.email || '';
  } catch {
    // profile fetch failed — leave fields empty
  }
}

let unlistenSignInPrompt: UnlistenFn | null = null;

onMounted(async () => {
  const session = await auth.checkSession();
  isSignedIn.value = !!session;
  if (isSignedIn.value) {
    await fetchUserProfile();
  }

  const store = await load('settings.json', { autoSave: true });
  const savedMode = await store.get<string>('recordingMode');
  if (savedMode === 'mic' || savedMode === 'mic_and_system') {
    recordingMode.value = savedMode;
  }

  unlistenSignInPrompt = await listen('tray://show-sign-in-prompt', () => {
    signInPrompt.value = true;
  });
});

onUnmounted(() => {
  unlistenSignInPrompt?.();
});

async function handleGoogleSignIn() {
  isSigningIn.value = true;
  errorMessage.value = '';
  try {
    const result = await auth.googleSignIn();
    if (result.error) {
      if (result.error !== 'Auth window closed') {
        errorMessage.value = result.error;
      }
      return;
    }
    isSignedIn.value = true;
    signInPrompt.value = false;
    await fetchUserProfile();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Sign in failed';
  } finally {
    isSigningIn.value = false;
  }
}

async function handleSignOut() {
  await auth.signOut();
  isSignedIn.value = false;
  displayName.value = '';
  email.value = '';
}
</script>

<style scoped>
.settings {
  padding: 24px;
  font-family: -apple-system, system-ui, sans-serif;
  background: #f5f5f7;
  min-height: 100vh;
}

.title {
  font-size: 20px;
  font-weight: 700;
  margin-bottom: 24px;
  color: #1d1d1f;
}

.signin-banner {
  background: #fef3c7;
  border: 1px solid #fcd34d;
  color: #92400e;
  font-size: 13px;
  font-weight: 500;
  padding: 10px 14px;
  border-radius: 10px;
  margin-bottom: 16px;
}

.section {
  margin-bottom: 20px;
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: #86868b;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
}

.card {
  background: white;
  border-radius: 10px;
  padding: 16px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
}

.account-info {
  display: flex;
  align-items: center;
  gap: 12px;
}

.avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: #6366f1;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
  color: white;
}

.account-details {
  display: flex;
  flex-direction: column;
  flex: 1;
}

.account-name {
  font-size: 14px;
  font-weight: 500;
  color: #1d1d1f;
}

.account-email {
  font-size: 12px;
  color: #86868b;
}

.sign-out-btn {
  font-size: 13px;
  color: #f87171;
  background: none;
  border: none;
  cursor: pointer;
  font-weight: 500;
}

.sign-out-btn:hover {
  text-decoration: underline;
}

.sign-in-container {
  text-align: center;
}

.google-btn {
  width: 100%;
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
  margin-top: 8px;
  font-size: 12px;
  color: #f87171;
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-label {
  font-size: 14px;
  color: #1d1d1f;
}

.setting-select {
  font-size: 13px;
  padding: 4px 8px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  background: white;
  color: #6366f1;
}

.about-text {
  font-size: 13px;
  color: #86868b;
}
</style>
