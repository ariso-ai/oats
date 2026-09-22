import { describe, it, expect } from 'vitest';
import { thumbGeometry, scrollTopForDrag } from './modelListScrollbar';

describe('thumbGeometry', () => {
  it('has no thumb when the list fits', () => {
    expect(thumbGeometry({ scrollTop: 0, scrollHeight: 300, clientHeight: 321 })).toBeNull();
    expect(thumbGeometry({ scrollTop: 0, scrollHeight: 321, clientHeight: 321 })).toBeNull();
  });

  it('sizes the thumb by how much of the list is on screen', () => {
    // Half the content visible, so the thumb covers half the track.
    const thumb = thumbGeometry({ scrollTop: 0, scrollHeight: 600, clientHeight: 300 });
    expect(thumb).toEqual({ height: 150, offset: 0 });
  });

  it('puts the thumb at the bottom of the track when scrolled to the end', () => {
    const { height, offset } = thumbGeometry({
      scrollTop: 300,
      scrollHeight: 600,
      clientHeight: 300,
    })!;
    expect(offset + height).toBe(300);
  });

  it('moves the thumb proportionally through the track', () => {
    const { offset, height } = thumbGeometry({
      scrollTop: 150,
      scrollHeight: 600,
      clientHeight: 300,
    })!;
    expect(offset).toBe((300 - height) / 2);
  });

  it('keeps a long list readable with a minimum thumb', () => {
    // 20 screens of content would otherwise leave a 15px sliver.
    const { height } = thumbGeometry({ scrollTop: 0, scrollHeight: 6000, clientHeight: 300 })!;
    expect(height).toBe(24);
  });

  it('stays inside the track when momentum scrolling overshoots', () => {
    const past = thumbGeometry({ scrollTop: 420, scrollHeight: 600, clientHeight: 300 })!;
    expect(past.offset + past.height).toBe(300);
    const before = thumbGeometry({ scrollTop: -40, scrollHeight: 600, clientHeight: 300 })!;
    expect(before.offset).toBe(0);
  });
});

describe('scrollTopForDrag', () => {
  const metrics = { scrollTop: 0, scrollHeight: 600, clientHeight: 300 };

  it('moves the list by the same fraction the thumb was dragged', () => {
    // Thumb is 150px in a 300px track, so 150px of travel covers 300px of
    // scrollable content: dragging half the travel scrolls half the content.
    const top = scrollTopForDrag({ startScrollTop: 0, deltaY: 75, metrics, thumbHeight: 150 });
    expect(top).toBe(150);
  });

  it('carries on from where the drag started', () => {
    const top = scrollTopForDrag({ startScrollTop: 100, deltaY: 25, metrics, thumbHeight: 150 });
    expect(top).toBe(150);
  });

  it('drags upward as well', () => {
    const top = scrollTopForDrag({ startScrollTop: 200, deltaY: -50, metrics, thumbHeight: 150 });
    expect(top).toBe(100);
  });

  it('stops at both ends however far the pointer goes', () => {
    expect(scrollTopForDrag({ startScrollTop: 0, deltaY: 999, metrics, thumbHeight: 150 })).toBe(
      300,
    );
    expect(scrollTopForDrag({ startScrollTop: 300, deltaY: -999, metrics, thumbHeight: 150 })).toBe(
      0,
    );
  });

  it('stays put when there is nowhere to scroll', () => {
    const fits = { scrollTop: 0, scrollHeight: 300, clientHeight: 300 };
    expect(scrollTopForDrag({ startScrollTop: 0, deltaY: 50, metrics: fits, thumbHeight: 300 })).toBe(
      0,
    );
  });
});
