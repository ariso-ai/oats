// Initials avatars shared by the detail panel's attendee list and its action
// items, so a person reads the same in both.

const AVATAR_COLORS = ['#6c63c0', '#0ea5e9', '#f59e0b', '#ec4899', '#22c55e', '#64748b'];

export function avatarColor(i: number): string {
  return AVATAR_COLORS[i % AVATAR_COLORS.length];
}

export function initials(name?: string): string {
  if (!name) return '?';
  return name.split(/\s+/).filter(Boolean).map((n) => n[0]).join('').toUpperCase().slice(0, 2) || '?';
}
