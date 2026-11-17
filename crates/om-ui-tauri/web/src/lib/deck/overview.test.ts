import { expect, test } from 'vitest';
import { overviewColumns, overviewLevel } from './overview';

const levels = [
  { samplesPerPoint: 256, len: 16 },
  { samplesPerPoint: 1024, len: 4 },
];

test('the coarsest level that still gives a point per pixel is used', () => {
  expect(overviewLevel(levels, 4096, 4)).toBe(1);
  expect(overviewLevel(levels, 4096, 16)).toBe(0);
  expect(overviewLevel([], 4096, 4)).toBe(-1);
  expect(overviewLevel(levels, 4096, 0)).toBe(-1);
});

test('each column takes the loudest body and its high band over the points it covers', () => {
  // Four points, (body, low, mid, high): body 51, 255, 0, 102 and high 255, 128, 0, 0.
  const bytes = new Uint8Array([51, 0, 0, 255, 255, 0, 0, 128, 0, 0, 0, 0, 102, 0, 0, 0]);
  const cols = overviewColumns(bytes, 0, 4, 1024, 4096, 2);
  expect(cols.body[0]).toBe(1);
  expect(cols.body[1]).toBeCloseTo(0.4);
  // High is a share of the body: 0.2 * 1 for the first point, 1 * 0.502 for the second.
  expect(cols.high[0]).toBeCloseTo(0.502, 3);
  expect(cols.high[1]).toBe(0);
});

test('no width or no points draws nothing', () => {
  expect(overviewColumns(new Uint8Array(0), 0, 0, 1024, 4096, 3).body).toEqual(new Float32Array(3));
  expect(overviewColumns(new Uint8Array(16), 0, 4, 1024, 4096, 0).body.length).toBe(0);
});
