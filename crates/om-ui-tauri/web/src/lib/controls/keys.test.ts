import { expect, test } from 'vitest';
import { keyValue, sliderKey } from './keys';

const plain = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };
const press = (key: string, position: number, mods: Partial<typeof plain> = {}) =>
  sliderKey({ ...plain, ...mods, key }, position);

test('arrows step one percent of the travel, up and right raising it', () => {
  expect(press('ArrowUp', 0.5)).toBeCloseTo(0.51);
  expect(press('ArrowRight', 0.5)).toBeCloseTo(0.51);
  expect(press('ArrowDown', 0.5)).toBeCloseTo(0.49);
  expect(press('ArrowLeft', 0.5)).toBeCloseTo(0.49);
});

test('Shift and the page keys step ten percent', () => {
  expect(press('ArrowUp', 0.5, { shiftKey: true })).toBeCloseTo(0.6);
  expect(press('ArrowLeft', 0.5, { shiftKey: true })).toBeCloseTo(0.4);
  expect(press('PageUp', 0.2)).toBeCloseTo(0.3);
  expect(press('PageDown', 0.2)).toBeCloseTo(0.1);
});

test('a step lands on the next mark, so the centre is never stepped over', () => {
  expect(press('ArrowUp', 0.495)).toBeCloseTo(0.5);
  expect(press('ArrowDown', 0.505)).toBeCloseTo(0.5);
  expect(press('ArrowUp', 0.43, { shiftKey: true })).toBeCloseTo(0.5);
  expect(press('ArrowDown', 0.57, { shiftKey: true })).toBeCloseTo(0.5);
});

test('every mark steps on: rounding in the division never sticks', () => {
  for (let n = 0; n < 100; n++) expect(press('ArrowUp', n / 100)).toBeCloseTo((n + 1) / 100);
  for (let n = 100; n > 0; n--) expect(press('ArrowDown', n / 100)).toBeCloseTo((n - 1) / 100);
});

test('steps stop at the ends', () => {
  expect(press('ArrowUp', 1)).toBe(1);
  expect(press('ArrowDown', 0)).toBe(0);
  expect(press('ArrowUp', 0.97, { shiftKey: true })).toBe(1);
  expect(press('PageDown', 0.04)).toBe(0);
});

test('Home and End go to the ends', () => {
  expect(press('Home', 0.37)).toBe(0);
  expect(press('End', 0.37)).toBe(1);
});

test('Delete and Backspace ask for the default', () => {
  expect(press('Delete', 0.37)).toBe('reset');
  expect(press('Backspace', 0.37)).toBe('reset');
});

test('other keys and chords with Ctrl, Alt or Meta are left to the window', () => {
  expect(press('a', 0.5)).toBeNull();
  expect(press('Tab', 0.5)).toBeNull();
  expect(press(' ', 0.5)).toBeNull();
  expect(press('ArrowUp', 0.5, { ctrlKey: true })).toBeNull();
  expect(press('ArrowUp', 0.5, { altKey: true })).toBeNull();
  expect(press('Home', 0.5, { metaKey: true })).toBeNull();
});

const LINEAR = { kind: 'linear' as const };
const channelFader = {
  min: 0,
  max: 1,
  detent: null,
  default: 1,
  law: {
    kind: 'knots' as const,
    knots: [
      [0, 0],
      [0.2, 0.004],
      [1, 1],
    ] as [number, number][],
  },
};
const eqLow = { min: -48, max: 6, detent: 0, default: 0, law: LINEAR };
const key = (k: string, mods: Partial<typeof plain> = {}) => ({ ...plain, ...mods, key: k });

test('a key steps the travel, so a tapered fader moves through its law', () => {
  // One percent of travel down from the top is 1/80 of the way along the top run.
  expect(keyValue(key('ArrowDown'), 1, channelFader)).toBeCloseTo(1 - 0.996 / 80, 12);
  expect(keyValue(key('ArrowUp', { shiftKey: true }), 0, channelFader)).toBeCloseTo(0.002, 12);
  expect(keyValue(key('End'), 0.3, channelFader)).toBe(1);
  expect(keyValue(key('Delete'), 0.3, channelFader)).toBe(1);
  expect(keyValue(key('Delete'), 0.3, channelFader, false)).toBeNull();
  expect(keyValue(key('a'), 0.3, channelFader)).toBeNull();
});

test('one arrow step leaves a knob detent, though the drag pull would hold it there', () => {
  const up = keyValue(key('ArrowUp'), 0, eqLow);
  const down = keyValue(key('ArrowDown'), 0, eqLow);
  expect(up).toBeCloseTo(0.12, 12);
  expect(down).toBeCloseTo(-0.96, 12);
  // Both are inside the 1.62 dB pull; keys do not snap, so they stay off the detent.
  expect(up !== null && Math.abs(up) < 1.62 && up !== 0).toBe(true);
  expect(down !== null && Math.abs(down) < 1.62 && down !== 0).toBe(true);
});
