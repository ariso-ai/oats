import './assets/main.css';
import { setupPluginListeners } from 'tauri-plugin-mcp';
import { createApp } from 'vue';
import { createRouter, createWebHashHistory } from 'vue-router';
import App from './App.vue';
import WaveformView from './views/WaveformView.vue';
import SettingsView from './views/SettingsView.vue';
import UpdateView from './views/UpdateView.vue';
import BootstrapView from './views/BootstrapView.vue';
import MeetingPickerView from './views/MeetingPickerView.vue';
import LibraryView from './views/LibraryView.vue';
import OnboardingView from './views/OnboardingView.vue';

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'Bootstrap', component: BootstrapView },
    { path: '/onboarding', name: 'Onboarding', component: OnboardingView },
    { path: '/waveform', name: 'Waveform', component: WaveformView },
    { path: '/settings', name: 'Settings', component: SettingsView },
    { path: '/update', name: 'Update', component: UpdateView },
    { path: '/meeting-picker', name: 'MeetingPicker', component: MeetingPickerView },
    { path: '/library', name: 'Library', component: LibraryView },
  ],
});

const app = createApp(App);
app.use(router);
app.mount('#app');

setupPluginListeners();
