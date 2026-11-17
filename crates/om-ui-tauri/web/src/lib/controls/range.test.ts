import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import type { ControlLaw } from '../ipc';
import { knotPosition, knotValue, toPosition, toValue, type Travel } from './range';

const LINEAR: ControlLaw = { kind: 'linear' };
const fader: Travel = { min: 0, max: 1, detent: null, law: LINEAR };
const trim: Travel = { min: 0, max: 4, detent: 1, law: LINEAR };
const tempo: Travel = { min: 0.92, max: 1.08, detent: null, law: LINEAR };

// A made-up taper: a ramp, then steeper and shallower runs, ending above 1.
const KNOTS: [number, number][] = [
  [0, 0],
  [0.2, 0.004],
  [0.5, 0.1],
  [0.75, 1],
  [1, 2],
];
const tapered: Travel = { min: 0, max: 2, detent: 1, law: { kind: 'knots', knots: KNOTS } };

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
  const point: Travel = { min: 2, max: 2, detent: null, law: LINEAR };
  expect(toPosition(2, point)).toBe(0);
  expect(toPosition(5, point)).toBe(0);
  expect(toValue(0.7, point)).toBe(2);
  // A detent on an end cannot split the travel; the map stays linear.
  expect(toPosition(0.5, { min: 0, max: 1, detent: 1, law: LINEAR })).toBeCloseTo(0.5);
});

test('a knotted law hits every knot and its midpoints, both ways', () => {
  for (const [pos, value] of KNOTS) {
    expect(toValue(pos, tapered)).toBe(value);
    expect(Math.abs(toPosition(value, tapered) - pos)).toBeLessThan(1e-9);
  }
  for (let i = 1; i < KNOTS.length; i++) {
    const [p0, v0] = KNOTS[i - 1];
    const [p1, v1] = KNOTS[i];
    const mid = (p0 + p1) / 2;
    expect(Math.abs(toValue(mid, tapered) - (v0 + v1) / 2)).toBeLessThan(1e-9);
    expect(Math.abs(toPosition(toValue(mid, tapered), tapered) - mid)).toBeLessThan(1e-9);
  }
});

test('a knotted law ignores the detent: the knots say where every value sits', () => {
  expect(toPosition(1, tapered)).toBe(0.75);
});

test('a knotted law rises with travel', () => {
  let last = -Infinity;
  for (let i = 0; i <= 1000; i++) {
    const v = toValue(i / 1000, tapered);
    expect(v).toBeGreaterThanOrEqual(last);
    last = v;
  }
});

test('a knotted law clamps outside 0..1 and outside its values', () => {
  expect(toValue(-0.5, tapered)).toBe(0);
  expect(toValue(1.5, tapered)).toBe(2);
  expect(toPosition(-1, tapered)).toBe(0);
  expect(toPosition(9, tapered)).toBe(1);
});

test('a degenerate table never divides by zero', () => {
  const one: [number, number][] = [[0.3, 0.5]];
  expect(knotValue(one, 0.9)).toBe(0.5);
  expect(knotPosition(one, 7)).toBe(0.3);
  const stacked: [number, number][] = [
    [0, 0],
    [0.5, 0.2],
    [0.5, 0.6],
    [1, 1],
  ];
  for (let i = 0; i <= 20; i++) expect(Number.isFinite(knotValue(stacked, i / 20))).toBe(true);
  expect(knotValue(stacked, 0.5)).toBe(0.2);
  expect(knotPosition(stacked, 0.4)).toBe(0.5);
  const flat: [number, number][] = [
    [0, 0.5],
    [1, 0.5],
  ];
  expect(knotPosition(flat, 0.5)).toBe(0);
  expect(knotValue([], 0.5)).toBe(0);
  expect(knotPosition([], 0.5)).toBe(0);
});

test('a snapshot value, stored as f32, puts the thumb back where the drag left it', () => {
  for (let i = 0; i < 20; i++) {
    const pos = (i + 0.5) / 20;
    const sent = toValue(pos, tapered);
    const back = toPosition(Math.fround(sent), tapered);
    expect(Math.abs(back - pos)).toBeLessThan(1e-6);
  }
});

// The table om-ui-bridge's `tests/fader_law_table.rs` also asserts against `om_ui::fader_law`.
const table = new TextDecoder()
  .decode(readFileSync(new URL('../../../../../../fixtures/ui-bridge/fader-law.txt', import.meta.url)))
  .split('\n')
  .filter((line) => line.trim() !== '' && !line.startsWith('#'))
  .map((line) => line.trim().split(/\s+/));

function tableLaw(key: string): Travel {
  const row = table.find((r) => r[0] === 'knots' && r[1] === key);
  if (!row) throw new Error(`no knots for ${key}`);
  const numbers = row.slice(2).map(Number);
  const knots: [number, number][] = [];
  for (let i = 0; i < numbers.length; i += 2) knots.push([numbers[i], numbers[i + 1]]);
  return { min: 0, max: knots[knots.length - 1][1], detent: null, law: { kind: 'knots', knots } };
}

// 0.01 dB: what om-ui-bridge guarantees between its 1% samples of the dB-linear law.
const MAX_ERROR_DB = 0.01;

test('every table position gives the om-ui law gain, within 0.01 dB', () => {
  const rows = table.filter((r) => r[0] !== 'knots');
  expect(rows.length).toBeGreaterThanOrEqual(10);
  for (const [key, pos, gain] of rows) {
    const got = toValue(Number(pos), tableLaw(key));
    const want = Number(gain);
    if (want === 0) expect(got, `${key} ${pos}`).toBe(0);
    else expect(Math.abs(20 * Math.log10(got / want)), `${key} ${pos}`).toBeLessThan(MAX_ERROR_DB);
  }
});

test('the table thumb positions survive the trip through the engine', () => {
  for (const key of ['ch0.fader', 'mixer.master']) {
    const law = tableLaw(key);
    for (let i = 0; i < 20; i++) {
      const pos = (i + 0.5) / 20;
      expect(Math.abs(toPosition(Math.fround(toValue(pos, law)), law) - pos), `${key} ${pos}`).toBeLessThan(1e-6);
    }
  }
});
