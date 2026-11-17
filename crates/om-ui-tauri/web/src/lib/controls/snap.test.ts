import { expect, test } from 'vitest';
import type { ControlSnap } from '../ipc';
import { dragSnap, snapToDetent } from './snap';

// The EQ's pull: 3% of its 54 dB either side of 0 dB.
const eq: ControlSnap = { at: 0, width: 1.62 };

test('inside the width a value lands on the detent, edges included', () => {
  expect(snapToDetent(1.2, eq, false)).toBe(0);
  expect(snapToDetent(-1.5, eq, false)).toBe(0);
  expect(snapToDetent(1.62, eq, false)).toBe(0);
  expect(snapToDetent(-1.62, eq, false)).toBe(0);
});

test('outside the width a value is left alone', () => {
  expect(snapToDetent(2, eq, false)).toBe(2);
  expect(snapToDetent(-30, eq, false)).toBe(-30);
});

test('Shift bypasses the pull', () => {
  expect(snapToDetent(1.2, eq, true)).toBe(1.2);
});

test('a control without a snap is left alone', () => {
  expect(snapToDetent(0.01, null, false)).toBe(0.01);
});

test('a drag pulls only once the value has moved off where the press found it', () => {
  expect(dragSnap(1, 1, eq, false)).toBe(1);
  expect(dragSnap(1.1, 1, eq, false)).toBe(0);
  expect(dragSnap(1.1, 1, eq, true)).toBe(1.1);
  expect(dragSnap(5, 1, eq, false)).toBe(5);
});
