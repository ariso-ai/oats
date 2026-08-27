/** Convert browser/native capture failures into a stable, actionable message.
 * Raw device errors vary by WebView2 and driver and should not be shown verbatim. */
export function recordingStartErrorMessage(error: unknown): string {
  const name =
    typeof error === 'object' && error !== null && 'name' in error
      ? String(error.name)
      : '';
  const detail =
    typeof error === 'string'
      ? error
      : typeof error === 'object' && error !== null && 'message' in error
        ? String(error.message)
        : '';

  if (name === 'NotAllowedError' || name === 'SecurityError') {
    return 'Microphone access is blocked. Enable microphone access for oats in Windows Settings, then try again.';
  }
  if (name === 'NotFoundError' || name === 'DevicesNotFoundError') {
    return 'No microphone was found. Connect or enable a microphone, then try again.';
  }
  if (
    name === 'NotReadableError'
    || name === 'TrackStartError'
    || name === 'AbortError'
  ) {
    return 'Oats could not start the microphone. Reconnect it or close other apps using it, then try again.';
  }
  if (name === 'OverconstrainedError') {
    return 'The selected microphone is not available in a supported format. Reconnect it or choose another input, then try again.';
  }
  if (/wasapi|system audio|output device|render endpoint/i.test(detail)) {
    return 'Oats could not start system audio. Connect or enable an output device, then try again.';
  }
  if (/sign-in required/i.test(detail)) {
    return 'Recording is not ready. Sign in or finish installing the local models in Settings, then try again.';
  }
  return 'Oats could not start recording. Check that your audio devices are connected and available, then try again.';
}

/** Why a queued recording never started: the recorder pill already up refused to
 * give up the one window slot. Mirrors the `reason` values the backend emits
 * from `open_waveform_window`'s yield-expiry branch. */
export type RecordingBlockedReason = 'capturing' | 'uploading';

export interface RecordingBlocked {
  reason: RecordingBlockedReason;
  /** The meeting holding the recorder, as a `MeetingListItem` id. Null when the
   * incumbent has no meeting attached (an ad-hoc local recording). */
  meetingId: string | null;
}

/** Read a `recording://start-failed` payload emitted by the yield-expiry branch.
 * Returns null for every other start failure — those carry a ready-made
 * `message` and are handled by `recordingStartErrorMessage`. */
export function recordingBlockedPayload(payload: unknown): RecordingBlocked | null {
  if (typeof payload !== 'object' || payload === null || !('reason' in payload)) return null;
  const { reason } = payload as { reason: unknown };
  if (reason !== 'capturing' && reason !== 'uploading') return null;
  const raw = 'meetingId' in payload ? (payload as { meetingId: unknown }).meetingId : null;
  const meetingId =
    typeof raw === 'number' && Number.isFinite(raw)
      ? String(raw)
      : typeof raw === 'string' && raw.length > 0
        ? raw
        : null;
  return { reason, meetingId };
}

/** Explain which recording is holding the recorder and what clears it, naming
 * the meeting when the caller could resolve a title. The two states are not
 * interchangeable — a pill mid-capture is not uploading, and telling the user to
 * wait for an upload that never runs leaves them stuck (#320). */
export function recordingBlockedMessage(
  reason: RecordingBlockedReason,
  title: string | null,
): string {
  const named = title !== null && title.trim().length > 0;
  if (reason === 'capturing') {
    return named
      ? `“${title!.trim()}” is still recording. Stop it before starting a new one.`
      : 'Another recording is still in progress. Stop it before starting a new one.';
  }
  return named
    ? `“${title!.trim()}” is still uploading. Try again once it finishes.`
    : 'The previous recording is still uploading. Try again once it finishes.';
}
