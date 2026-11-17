import { expect, test } from 'vitest';
import { CLICK_SLOP_PX, FINE_DIVISOR, Held, PressTravel, faderValue, knobDelta } from './drag';

test('knobDelta', () => {
  expect(knobDelta(0.5, -16)).toBeCloseTo(0.6);
  expect(knobDelta(0.95, -160)).toBe(1);
  expect(knobDelta(0.05, 160)).toBe(0);
});

test('Shift held at the press makes a knob drag ten times finer, as in GPUI', () => {
  expect(FINE_DIVISOR).toBe(10);
  // The same 16 px up: a tenth of the travel.
  expect(knobDelta(0.5, -16, true)).toBeCloseTo(0.51);
  expect(knobDelta(0.5, -16, false) - 0.5).toBeCloseTo(10 * (knobDelta(0.5, -16, true) - 0.5));
  expect(knobDelta(0.5, 16, true)).toBeCloseTo(0.49);
  expect(knobDelta(0.995, -160, true)).toBe(1);
  // Without Shift nothing changes.
  expect(knobDelta(0.5, -16, false)).toBe(knobDelta(0.5, -16));
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

test('a press that stays within the slop is a click; one that left it is a drag', () => {
  const travel = new PressTravel();
  travel.begin(100, 200);
  travel.move(102, 201);
  expect(travel.dragged).toBe(false);
  travel.move(100, 200 - CLICK_SLOP_PX - 1);
  expect(travel.dragged).toBe(true);
  // Coming back does not make it a click again.
  travel.move(100, 200);
  expect(travel.dragged).toBe(true);
  // The next press starts clean.
  travel.begin(10, 10);
  expect(travel.dragged).toBe(false);
});
