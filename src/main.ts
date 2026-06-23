import './assets/main.css';
import { setupPluginListeners } from 'tauri-plugin-mcp';
import { createApp } from 'vue';
import { createRouter, createWebHashHistory } from 'vue-router';
import App from './App.vue';

// Routes are lazy-loaded so each window only fetches the view it renders, and a
// broken import in one view stays contained to its own route instead of taking
// down the whole app.
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'Bootstrap', component: () => import('./views/BootstrapView.vue') },
    { path: '/onboarding', name: 'Onboarding', component: () => import('./views/OnboardingView.vue') },
    { path: '/waveform', name: 'Waveform', component: () => import('./views/WaveformView.vue') },
    { path: '/settings', name: 'Settings', component: () => import('./views/SettingsView.vue') },
    { path: '/update', name: 'Update', component: () => import('./views/UpdateView.vue') },
    { path: '/meeting-picker', name: 'MeetingPicker', component: () => import('./views/MeetingPickerView.vue') },
    { path: '/meeting-prompt', name: 'MeetingPrompt', component: () => import('./views/MeetingPromptView.vue') },
    { path: '/silence-prompt', name: 'SilencePrompt', component: () => import('./views/SilencePromptView.vue') },
    { path: '/library', name: 'Library', component: () => import('./views/LibraryView.vue') },
  ],
});

const app = createApp(App);
app.use(router);
app.mount('#app');

setupPluginListeners();
