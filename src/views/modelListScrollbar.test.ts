import { describe, it, expect } from 'vitest';
import { thumbGeometry } from './modelListScrollbar';

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
