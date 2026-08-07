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
  if (name === 'SelectedAudioInputUnavailableError') {
    return 'The selected microphone is unavailable. Reconnect it or choose another input in Settings, then try again.';
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
