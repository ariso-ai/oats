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

export interface DragToScroll {
  /** Where the list was scrolled when the drag began. */
  startScrollTop: number;
  /** How far the pointer has moved since, in px. */
  deltaY: number;
  metrics: ScrollMetrics;
  /** The thumb's height, which sets how much track the drag has to work with. */
  thumbHeight: number;
  /** The track's own height; defaults to the scroller's viewport. */
  trackLength?: number;
}

/**
 * Where to scroll the list to for a thumb dragged `deltaY` from its start.
 *
 * The thumb covers the track in proportion to what is on screen, so a pixel of
 * travel is worth more than a pixel of content: the ratio is the scrollable
 * height over the track's own length.
 */
export function scrollTopForDrag({
  startScrollTop,
  deltaY,
  metrics,
  thumbHeight,
  trackLength = metrics.clientHeight,
}: DragToScroll): number {
  const scrollable = metrics.scrollHeight - metrics.clientHeight;
  const travel = trackLength - thumbHeight;
  if (scrollable <= 0 || travel <= 0) return startScrollTop;
  const next = startScrollTop + (deltaY * scrollable) / travel;
  return Math.min(scrollable, Math.max(0, next));
}

/**
 * Where the thumb sits for a given scroll position, or `null` when everything
 * fits and no scrollbar belongs on screen.
 *
 * `trackLength` is the bar's own height, which is not the scroller's: the track
 * starts below the sticky header. Sizing against the viewport instead would
 * hang the thumb past the bottom of the track — and of the card — at the end of
 * the list.
 */
export function thumbGeometry(
  { scrollTop, scrollHeight, clientHeight }: ScrollMetrics,
  trackLength: number = clientHeight,
): ThumbGeometry | null {
  if (!(scrollHeight > clientHeight) || clientHeight <= 0 || trackLength <= 0) return null;

  const proportional = (clientHeight / scrollHeight) * trackLength;
  const height = Math.min(trackLength, Math.max(MIN_THUMB_PX, proportional));
  const travel = trackLength - height;
  const scrollable = scrollHeight - clientHeight;
  // Clamp: momentum scrolling overshoots both ends, and a thumb that runs past
  // the track reads as a rendering bug.
  const progress = Math.min(1, Math.max(0, scrollTop / scrollable));
  return { height, offset: progress * travel };
}
