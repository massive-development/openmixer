import { expect, test } from 'vitest';
import { toPosition, toValue } from './range';

const fader = { min: 0, max: 1, detent: null };
const trim = { min: 0, max: 4, detent: 1 };
const tempo = { min: 0.92, max: 1.08, detent: null };

test('a plain range maps linearly both ways', () => {
  expect(toPosition(0.25, fader)).toBeCloseTo(0.25);
  expect(toValue(0.25, fader)).toBeCloseTo(0.25);
  expect(toPosition(1, tempo)).toBeCloseTo(0.5);
  expect(toValue(1, tempo)).toBeCloseTo(1.08);
});

test('a detent sits at the middle of the travel, each side linear', () => {
  expect(toPosition(1, trim)).toBeCloseTo(0.5);
  expect(toPosition(0.5, trim)).toBeCloseTo(0.25);
  expect(toPosition(2.5, trim)).toBeCloseTo(0.75);
  expect(toValue(0.75, trim)).toBeCloseTo(2.5);
  expect(toValue(toPosition(3.2, trim), trim)).toBeCloseTo(3.2);
});

test('out-of-range input clamps', () => {
  expect(toPosition(9, trim)).toBe(1);
  expect(toPosition(-1, fader)).toBe(0);
  expect(toValue(1.5, tempo)).toBeCloseTo(1.08);
  expect(toValue(-0.5, trim)).toBe(0);
});

test('an empty range is position 0 and its one value', () => {
  const point = { min: 2, max: 2, detent: null };
  expect(toPosition(2, point)).toBe(0);
  expect(toPosition(5, point)).toBe(0);
  expect(toValue(0.7, point)).toBe(2);
  // A detent on an end cannot split the travel; the map stays linear.
  expect(toPosition(0.5, { min: 0, max: 1, detent: 1 })).toBeCloseTo(0.5);
});
