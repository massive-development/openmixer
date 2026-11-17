import { expect, test } from 'vitest';
import { layoutMap, type Placed } from './drive';

const placed = (key: string | null, left: number, top: number, width = 10, height = 20): Placed => ({
  getAttribute: (name) => (name === 'data-control' ? key : null),
  getBoundingClientRect: () => ({ left, top, width, height }),
});

const view = { innerWidth: 1600, innerHeight: 780, devicePixelRatio: 1.25 };

test('every keyed element is mapped by its key, in CSS px', () => {
  const map = layoutMap([placed('deck0.play', 12, 700, 80, 40), placed('ch1.eq.low', 900.5, 520)], view);
  expect(map.controls['deck0.play']).toEqual({ x: 12, y: 700, width: 80, height: 40 });
  expect(map.controls['ch1.eq.low']).toEqual({ x: 900.5, y: 520, width: 10, height: 20 });
  expect(map.devicePixelRatio).toBe(1.25);
  expect(map.viewport).toEqual({ width: 1600, height: 780 });
});

test('an element without a key is skipped and the first of a key wins', () => {
  const map = layoutMap([placed(null, 0, 0), placed('', 1, 1), placed('k', 5, 5), placed('k', 9, 9)], view);
  expect(Object.keys(map.controls)).toEqual(['k']);
  expect(map.controls.k.x).toBe(5);
});

test('a window without a scale reads as 1', () => {
  expect(layoutMap([], { ...view, devicePixelRatio: 0 }).devicePixelRatio).toBe(1);
});
