import { expect, test } from 'vitest';
import { Held, faderValue, knobDelta } from './drag';

test('knobDelta', () => {
  expect(knobDelta(0.5, -16)).toBeCloseTo(0.6);
  expect(knobDelta(0.95, -160)).toBe(1);
  expect(knobDelta(0.05, 160)).toBe(0);
});

test('faderValue', () => {
  expect(faderValue(150, 100, 200, false)).toBeCloseTo(0.25);
  expect(faderValue(150, 100, 200, true)).toBeCloseTo(0.75);
  expect(faderValue(50, 100, 200, false)).toBe(0);
  expect(faderValue(999, 100, 200, false)).toBe(1);
  expect(faderValue(150, 100, 0, false)).toBe(0);
});

test('a held value shows while dragged and until the engine has caught up', () => {
  const held = new Held();
  expect(held.shown(0.4)).toBe(0.4);
  held.begin(0.4);
  held.set(0.7);
  expect(held.shown(0.4)).toBe(0.7);
  held.onSnapshot();
  held.onSnapshot();
  expect(held.shown(0.4)).toBe(0.7);
  held.end();
  held.onSnapshot();
  expect(held.shown(0.5)).toBe(0.7);
  held.onSnapshot();
  expect(held.shown(0.69)).toBe(0.69);
});
