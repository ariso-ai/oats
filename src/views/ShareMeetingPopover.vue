<template>
  <teleport to="body">
    <div class="share-overlay" @click="emit('close')" />
    <div class="share-pop" :style="panelStyle">
      <div class="share-head">
        <h3>Share notes</h3>
      </div>

      <div class="share-body">
        <!-- Inline unshare confirm -->
        <div v-if="unshareTarget" class="unshare-confirm">
          <span class="unshare-text">Unshare with {{ unshareTarget }}?</span>
          <div class="unshare-actions">
            <button class="btn-ghost" type="button" :disabled="unsharing" @click="cancelUnshare">Cancel</button>
            <button class="btn-danger" type="button" :disabled="unsharing" @click="confirmUnshare">
              {{ unsharing ? 'Removing…' : 'Unshare' }}
            </button>
          </div>
        </div>

        <!-- Shared participant avatars -->
        <div class="avatar-grid">
          <div v-for="(p, i) in participants" :key="p.id ?? i" class="avatar-cell" :title="p.email || ''">
            <button
              type="button"
              class="avatar-lg"
              :class="{ shared: isEmailShared(p.email) }"
              :style="{ background: avatarColor(i) }"
              :disabled="!isEmailShared(p.email)"
              @click.stop="p.email && startUnshare(p.email)"
            >{{ initials(p.name) }}</button>
            <span class="avatar-name">{{ p.name?.split(' ')[0] || 'Guest' }}</span>
          </div>
          <div v-for="email in extraSharedEmails" :key="email" class="avatar-cell" :title="`Shared with ${email}`">
            <button type="button" class="avatar-lg shared" @click.stop="startUnshare(email)">
              {{ email[0]?.toUpperCase() || '@' }}
            </button>
            <span class="avatar-name">{{ email.split('@')[0] }}</span>
          </div>
        </div>

        <!-- Share by email -->
        <div class="divider" />
        <div class="email-row">
          <input
            v-model="shareEmailInput"
            type="email"
            placeholder="Enter email address"
            class="email-input"
            @keydown.enter.prevent="sendEmail"
          />
          <button class="btn-secondary" type="button" :disabled="shareEmailSending || !shareEmailInput.trim()" @click="sendEmail">
            {{ shareEmailSending ? 'Sending…' : 'Send' }}
          </button>
        </div>
        <p v-if="shareEmailError" class="err">{{ shareEmailError }}</p>

        <!-- Visibility -->
        <div class="divider" />
        <div class="vis-row">
          <button class="vis-toggle" type="button" @click="showSharingOptions = !showSharingOptions">
            <span v-if="sharingOption === 'public'">Anyone with the link</span>
            <span v-else-if="sharingOption === 'workspace'">Everyone at org with the link</span>
            <span v-else>Only people invited</span>
            <svg viewBox="0 0 24 24" class="ic"><path d="M19 9l-7 7-7-7" /></svg>
          </button>
          <button
            v-if="(sharingOption === 'workspace' || sharingOption === 'public') && existingShareUrl"
            class="copy-link"
            type="button"
            @click="copyLink"
          >{{ linkCopied ? 'Copied!' : 'Copy link' }}</button>
        </div>
        <div v-if="showSharingOptions" class="vis-menu">
          <button class="vis-item" type="button" :disabled="!canSharePublic" @click="canSharePublic && selectSharingOption('public')">
            <span>Anyone with the link</span>
            <svg v-if="sharingOption === 'public'" viewBox="0 0 24 24" class="ic check"><path d="M5 13l4 4L19 7" /></svg>
          </button>
          <button class="vis-item" type="button" @click="selectSharingOption('workspace')">
            <span>Everyone at org with the link</span>
            <svg v-if="sharingOption === 'workspace'" viewBox="0 0 24 24" class="ic check"><path d="M5 13l4 4L19 7" /></svg>
          </button>
          <button class="vis-item" type="button" @click="selectSharingOption('private')">
            <span>Only people invited</span>
            <svg v-if="sharingOption === 'private'" viewBox="0 0 24 24" class="ic check"><path d="M5 13l4 4L19 7" /></svg>
          </button>
        </div>

        <!-- Public expiry picker -->
        <div v-if="sharingOption === 'public'" class="expiry">
          <div class="expiry-row">
            <label for="pub-days">Link expires in</label>
            <div class="expiry-input-wrap">
              <input
                id="pub-days"
                :value="publicShareExpiresInDays"
                type="text"
                inputmode="numeric"
                maxlength="3"
                class="expiry-input"
                @input="onDayInput"
                @focus="showDayOptions = true"
                @blur="onDayBlur"
              />
              <span class="expiry-unit">days</span>
              <div v-if="showDayOptions" class="day-menu">
                <button
                  v-for="d in PUBLIC_SHARE_DAY_OPTIONS"
                  :key="d"
                  type="button"
                  class="day-opt"
                  :class="{ active: publicShareExpiresInDays === d }"
                  @mousedown.prevent="selectDayOption(d)"
                >{{ d }} days</button>
              </div>
            </div>
            <button class="btn-secondary" type="button" :disabled="sharing || !isDayValid" @click="confirmPublic">
              {{ sharing ? 'Saving…' : 'Save' }}
            </button>
          </div>
          <p v-if="!isDayValid" class="err">Enter a value between 1 and 365 days.</p>
          <p v-if="detail.visibility === 'public' && publicShareExpiryLabel" class="hint">
            Currently public until {{ publicShareExpiryLabel }}.
          </p>
          <p class="hint">The meeting assessment will not be shared publicly.</p>
        </div>

        <div v-if="shareError" class="err-box">{{ shareError }}</div>
      </div>
    </div>
  </teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { getDesktopConfig } from '../tauri';
import { useMeetingApi } from '../composables/useMeetingApi';
import type { MeetingDetail } from '../composables/useBackend';

type Visibility = 'private' | 'workspace' | 'public';
interface AnchorRect { bottom: number; right: number }

const props = defineProps<{ detail: MeetingDetail; meetingId: string; anchor: AnchorRect | null }>();
const emit = defineEmits<{ close: [] }>();

const api = useMeetingApi();

const PUBLIC_SHARE_DAY_OPTIONS = [7, 14, 30, 60, 90] as const;
const DEFAULT_PUBLIC_SHARE_DAYS = 30;
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const webAppBaseUrl = ref('');
const sentShareEmails = ref<Set<string>>(new Set());
const shareEmailInput = ref('');
const shareEmailSending = ref(false);
const shareEmailError = ref('');
const showSharingOptions = ref(false);
const sharingOption = ref<Visibility>((props.detail.visibility as Visibility) || 'private');
const sharing = ref(false);
const shareError = ref('');
const shareUrl = ref<string | null>(null);
const linkCopied = ref(false);
const publicShareExpiresInDays = ref<number>(DEFAULT_PUBLIC_SHARE_DAYS);
const showDayOptions = ref(false);
const unshareTarget = ref<string | null>(null);
const unsharing = ref(false);

const panelStyle = computed<Record<string, string>>(() => {
  const a = props.anchor;
  if (!a) return { position: 'fixed', top: '64px', right: '24px', width: '360px' };
  const width = 360;
  const left = Math.max(8, Math.min(a.right - width, window.innerWidth - width - 8));
  return { position: 'fixed', top: `${a.bottom + 6}px`, left: `${left}px`, width: `${width}px` };
});

const participants = computed(() => props.detail.participants ?? []);
const isHost = computed(() => participants.value.some((p) => p.role === 'host' && p.self));
const isAttendee = computed(() => participants.value.some((p) => p.role !== 'host' && p.self));

const canSharePublic = computed(() => {
  const setting = props.detail.shareMeetingNotesToPublic || 'off';
  if (setting === 'off') return false;
  if (setting === 'attendee_and_host') return isHost.value || isAttendee.value;
  if (setting === 'host_only') return isHost.value;
  return false;
});

const existingShareUrl = computed<string | null>(() => {
  const code = props.detail.shortCode;
  if (!code || !webAppBaseUrl.value) return null;
  return props.detail.visibility === 'public'
    ? `${webAppBaseUrl.value}/shared/meeting-notes/${code}`
    : `${webAppBaseUrl.value}/meeting-notes/${code}`;
});

const extraSharedEmails = computed(() =>
  [...sentShareEmails.value].filter(
    (email) => !participants.value.some((p) => p.email?.toLowerCase() === email)
  )
);

const isDayValid = computed(() => {
  const d = publicShareExpiresInDays.value;
  return Number.isInteger(d) && d >= 1 && d <= 365;
});

const publicShareExpiryLabel = computed(() => {
  const iso = props.detail.publicShareExpiresAt;
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
});

function isEmailShared(email?: string): boolean {
  return !!email && sentShareEmails.value.has(email.toLowerCase());
}
function initials(name?: string): string {
  if (!name) return 'G';
  return name.split(/\s+/).filter(Boolean).map((n) => n[0]).join('').toUpperCase().slice(0, 2) || 'G';
}
const AVATAR_COLORS = ['#6c63c0', '#0ea5e9', '#f59e0b', '#ec4899', '#22c55e', '#64748b'];
function avatarColor(i: number): string {
  return AVATAR_COLORS[i % AVATAR_COLORS.length];
}

function syncPublicShareDays(): void {
  const iso = props.detail.publicShareExpiresAt;
  if (props.detail.visibility === 'public' && iso) {
    const remainingMs = new Date(iso).getTime() - Date.now();
    publicShareExpiresInDays.value = Math.max(1, Math.round(remainingMs / 86400000));
  } else {
    publicShareExpiresInDays.value = DEFAULT_PUBLIC_SHARE_DAYS;
  }
}
function onDayInput(e: Event): void {
  const parsed = Number.parseInt((e.target as HTMLInputElement).value, 10);
  publicShareExpiresInDays.value = Number.isFinite(parsed) ? parsed : 0;
}
function selectDayOption(d: number): void {
  publicShareExpiresInDays.value = d;
  showDayOptions.value = false;
}
function onDayBlur(): void {
  setTimeout(() => (showDayOptions.value = false), 150);
}

async function selectSharingOption(option: Visibility): Promise<void> {
  sharingOption.value = option;
  showSharingOptions.value = false;
  if (option === 'public') {
    syncPublicShareDays();
    showDayOptions.value = false;
    return;
  }
  await doShare(option);
}

async function doShare(visibility: Visibility, expiresInDays?: number): Promise<void> {
  sharing.value = true;
  shareError.value = '';
  try {
    const r = await api.shareMeeting(props.meetingId, visibility, expiresInDays);
    if (r.shortCode) props.detail.shortCode = r.shortCode;
    props.detail.visibility = visibility;
    props.detail.publicShareExpiresAt = r.publicShareExpiresAt;
    shareUrl.value = r.shareUrl;
  } catch (e) {
    shareError.value = e instanceof Error ? e.message : 'Failed to share meeting notes';
  } finally {
    sharing.value = false;
  }
}

async function confirmPublic(): Promise<void> {
  await doShare('public', publicShareExpiresInDays.value);
}

async function copyLink(): Promise<void> {
  const url = existingShareUrl.value;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    linkCopied.value = true;
    setTimeout(() => (linkCopied.value = false), 2000);
  } catch {
    alert('Copy this link: ' + url);
  }
}

async function sendEmail(): Promise<void> {
  const email = shareEmailInput.value.trim().toLowerCase();
  if (!email) return;
  shareEmailError.value = '';
  if (!EMAIL_PATTERN.test(email)) {
    shareEmailError.value = 'Please enter a valid email address';
    return;
  }
  shareEmailSending.value = true;
  try {
    const r = await api.sendShareEmail(props.meetingId, email);
    sentShareEmails.value = new Set(sentShareEmails.value).add(email);
    shareEmailInput.value = '';
    if (r.alreadyShared) shareEmailError.value = 'Already shared with this email';
  } catch (e) {
    shareEmailError.value = e instanceof Error ? e.message : 'Failed to send email';
  } finally {
    shareEmailSending.value = false;
  }
}

function startUnshare(email: string): void {
  unshareTarget.value = email.toLowerCase();
}
function cancelUnshare(): void {
  if (!unsharing.value) unshareTarget.value = null;
}
async function confirmUnshare(): Promise<void> {
  const email = unshareTarget.value;
  if (!email) return;
  unsharing.value = true;
  shareEmailError.value = '';
  try {
    await api.unshareEmail(props.meetingId, email);
    const next = new Set(sentShareEmails.value);
    next.delete(email);
    sentShareEmails.value = next;
  } catch (e) {
    shareEmailError.value = e instanceof Error ? e.message : 'Failed to unshare';
  } finally {
    unsharing.value = false;
    unshareTarget.value = null;
  }
}

onMounted(async () => {
  try {
    const cfg = await getDesktopConfig();
    webAppBaseUrl.value = cfg.webAppBaseUrl;
  } catch {
    // Non-fatal: copy-link just stays hidden without a base URL.
  }
  const emails = await api.listShareEmails(props.meetingId);
  sentShareEmails.value = new Set(emails.map((e) => e.toLowerCase()));
  if (props.detail.visibility === 'public') syncPublicShareDays();
});
</script>

<style scoped>
.share-overlay { position: fixed; inset: 0; z-index: 60; }
.share-pop {
  z-index: 61; background: #fff; border: 1px solid #e5e6e3; border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.12); font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c; max-height: 80vh; overflow-y: auto;
}
.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
.share-head { padding: 12px 16px; border-bottom: 1px solid #e5e6e3; }
.share-head h3 { margin: 0; font-size: 15px; font-weight: 600; }
.share-body { padding: 16px; display: flex; flex-direction: column; gap: 14px; }
.divider { height: 1px; background: #e5e6e3; }

.unshare-confirm { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 10px; background: #fef2f2; border: 1px solid #fecaca; border-radius: 8px; font-size: 13px; }
.unshare-actions { display: flex; gap: 6px; flex-shrink: 0; }

.avatar-grid { display: flex; flex-wrap: wrap; gap: 14px; justify-content: center; }
.avatar-cell { display: flex; flex-direction: column; align-items: center; gap: 6px; max-width: 64px; }
.avatar-lg { width: 48px; height: 48px; border-radius: 50%; border: 2px solid #d6d6d6; color: #fff; font-size: 15px; font-weight: 600; display: flex; align-items: center; justify-content: center; cursor: default; }
.avatar-lg.shared { border-color: #22c55e; cursor: pointer; }
.avatar-lg.shared:hover { border-color: #ef4444; }
.avatar-name { font-size: 11px; color: #6f6f6f; text-align: center; max-width: 60px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.email-row { display: flex; gap: 8px; }
.email-input { flex: 1; min-width: 0; height: 34px; padding: 0 10px; border: 1px solid #d6d6d6; border-radius: 8px; font-family: inherit; font-size: 13px; }
.email-input:focus { outline: none; border-color: #6c63c0; }

.vis-row { display: flex; align-items: center; justify-content: space-between; }
.vis-toggle { display: flex; align-items: center; gap: 6px; background: none; border: none; font-family: inherit; font-size: 13px; color: #535353; cursor: pointer; }
.copy-link { background: none; border: none; font-family: inherit; font-size: 13px; font-weight: 600; color: #15803d; cursor: pointer; }
.vis-menu { border: 1px solid #e5e6e3; border-radius: 8px; overflow: hidden; }
.vis-item { width: 100%; display: flex; align-items: center; justify-content: space-between; padding: 9px 12px; background: #fff; border: none; font-family: inherit; font-size: 13px; color: #1c1c1c; cursor: pointer; }
.vis-item:hover:not(:disabled) { background: #faf9f7; }
.vis-item:disabled { opacity: 0.5; cursor: not-allowed; }
.check { color: #16a34a; }

.expiry { display: flex; flex-direction: column; gap: 6px; }
.expiry-row { display: flex; align-items: center; gap: 8px; }
.expiry-row label { font-size: 13px; font-weight: 500; color: #535353; white-space: nowrap; }
.expiry-input-wrap { position: relative; flex: 1; min-width: 0; }
.expiry-input { width: 100%; height: 34px; padding: 0 44px 0 10px; border: 1px solid #d6d6d6; border-radius: 8px; font-family: inherit; font-size: 13px; }
.expiry-input:focus { outline: none; border-color: #6c63c0; }
.expiry-unit { position: absolute; right: 10px; top: 0; height: 34px; display: flex; align-items: center; font-size: 13px; color: #6f6f6f; pointer-events: none; }
.day-menu { position: absolute; left: 0; right: 0; top: 38px; background: #fff; border: 1px solid #e5e6e3; border-radius: 8px; box-shadow: 0 6px 18px rgba(0,0,0,0.12); z-index: 5; max-height: 180px; overflow-y: auto; }
.day-opt { width: 100%; padding: 8px 12px; text-align: left; background: #fff; border: none; font-family: inherit; font-size: 13px; color: #535353; cursor: pointer; }
.day-opt:hover, .day-opt.active { background: #f0eeed; color: #1c1c1c; }

.btn-secondary { height: 34px; padding: 0 14px; background: #fff; border: 1px solid #d6d6d6; border-radius: 8px; box-shadow: 2px 2px 0 #e7e5e2; font-family: inherit; font-size: 13px; font-weight: 600; color: #1a1a1a; cursor: pointer; white-space: nowrap; }
.btn-secondary:disabled { opacity: 0.55; cursor: not-allowed; }
.btn-ghost { height: 28px; padding: 0 10px; background: none; border: 1px solid #d6d6d6; border-radius: 6px; font-family: inherit; font-size: 12px; cursor: pointer; }
.btn-danger { height: 28px; padding: 0 10px; background: #ef4444; border: none; border-radius: 6px; color: #fff; font-family: inherit; font-size: 12px; font-weight: 600; cursor: pointer; }
.btn-danger:disabled { opacity: 0.6; cursor: not-allowed; }

.err { margin: 0; font-size: 12px; color: #dc2626; }
.hint { margin: 0; font-size: 12px; color: #9a9a9a; }
.err-box { padding: 8px 10px; background: #fef2f2; border: 1px solid #fecaca; border-radius: 8px; font-size: 13px; color: #b91c1c; }
</style>
