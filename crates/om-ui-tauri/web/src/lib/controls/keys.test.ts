import { expect, test } from 'vitest';
import { sliderKey } from './keys';

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
