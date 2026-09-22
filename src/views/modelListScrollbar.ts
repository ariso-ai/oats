/**
 * Geometry for the AI-models list's own scrollbar.
 *
 * The list draws its own indicator rather than styling the native one: this
 * webview fixes a custom scrollbar's appearance when the scroller is created
 * and does not repaint it when the style changes afterwards, so a bar that is
 * transparent until hover can never become visible. Measured in the running
 * app — a `:hover` rule and a JS-toggled class both applied and both failed to
 * repaint, while the same rule present at first render drew fine.
 */

/** What the scroll container reports. */
export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

export interface ThumbGeometry {
  /** Thumb height in px. */
  height: number;
  /** Distance from the track's top in px. */
  offset: number;
}

/** Short enough to stay a thumb rather than a line, on a long list. */
const MIN_THUMB_PX = 24;

/**
 * Where the thumb sits for a given scroll position, or `null` when everything
 * fits and no scrollbar belongs on screen.
 */
export function thumbGeometry({
  scrollTop,
  scrollHeight,
  clientHeight,
}: ScrollMetrics): ThumbGeometry | null {
  if (!(scrollHeight > clientHeight) || clientHeight <= 0) return null;

  const proportional = (clientHeight / scrollHeight) * clientHeight;
  const height = Math.min(clientHeight, Math.max(MIN_THUMB_PX, proportional));
  const travel = clientHeight - height;
  const scrollable = scrollHeight - clientHeight;
  // Clamp: momentum scrolling overshoots both ends, and a thumb that runs past
  // the track reads as a rendering bug.
  const progress = Math.min(1, Math.max(0, scrollTop / scrollable));
  return { height, offset: progress * travel };
}
