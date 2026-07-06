// Read the picker's `defaultMeetingId` from a location hash like
// `#/meeting-picker?defaultMeetingId=50`. Router-free so the picker (which does
// not use vue-router) stays testable via window.location.hash.
export function parseDefaultMeetingId(hash: string): number | null {
  const qIndex = hash.indexOf('?');
  if (qIndex < 0) return null;
  const params = new URLSearchParams(hash.slice(qIndex + 1));
  const raw = params.get('defaultMeetingId');
  if (raw == null || !/^\d+$/.test(raw)) return null;
  const n = Number(raw);
  return Number.isSafeInteger(n) ? n : null;
}
