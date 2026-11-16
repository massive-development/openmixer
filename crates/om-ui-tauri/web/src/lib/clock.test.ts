import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { interpolate } from './clock';
import { decodeSnapshot } from './packet';

const golden = readFileSync(new URL('../../../../../fixtures/ui-bridge/snapshot-v2.bin', import.meta.url));
const buf = golden.buffer.slice(golden.byteOffset, golden.byteOffset + golden.byteLength) as ArrayBuffer;

test('interpolates a playing deck', () => {
  const s = decodeSnapshot(buf, 1000);
  expect(interpolate(s.decks[0], s, 1010)).toBeCloseTo(441000 + (12.5 / 1000) * 44100 * 1.02, 3);
});

test('interpolate_clamps', () => {
  const s = decodeSnapshot(buf, 0);
  expect(interpolate({ ...s.decks[0], position: -500 }, s, 0)).toBe(0);
  expect(interpolate({ ...s.decks[0], position: 11025000 }, s, 60000)).toBe(11024999);
  expect(interpolate({ ...s.decks[0], length: 0, position: 10 }, s, 0)).toBe(0);
});

test('a paused deck does not advance', () => {
  const s = decodeSnapshot(buf, 0);
  expect(interpolate({ ...s.decks[0], playing: false }, s, 5000)).toBe(441000);
});

// Deck 0 of the golden packet plays at 44100 * 1.02 frames a second round 440000..450000 from 441000.
const RATE = 44100 * 1.02;

test('wraps once at the loop end', () => {
  const s = decodeSnapshot(buf, 0);
  const at = interpolate(s.decks[0], s, 300 - s.ageMs);
  expect(441000 + 0.3 * RATE).toBeGreaterThan(450000);
  expect(at).toBeCloseTo(441000 + 0.3 * RATE - 10000, 3);
});

test('wraps several times round a very short loop over a long gap', () => {
  const s = decodeSnapshot(buf, 0);
  const deck = { ...s.decks[0], loopStart: 441000, loopEnd: 441100 };
  const at = interpolate(deck, s, 5000 - s.ageMs);
  expect(at).toBeCloseTo(441000 + ((5 * RATE) % 100), 3);
});

test('does not wrap when the deck is not looping', () => {
  const s = decodeSnapshot(buf, 0);
  const at = interpolate({ ...s.decks[0], looping: false }, s, 200 - s.ageMs);
  expect(at).toBeCloseTo(441000 + 0.2 * RATE, 3);
});

test('does not wrap when the position starts outside the loop', () => {
  const s = decodeSnapshot(buf, 0);
  const deck = { ...s.decks[0], position: 430000 };
  expect(interpolate(deck, s, 1000 - s.ageMs)).toBeCloseTo(430000 + RATE, 3);
});

test('ignores an invalid loop', () => {
  const s = decodeSnapshot(buf, 0);
  const deck = { ...s.decks[0], loopStart: 450000, loopEnd: 450000 };
  expect(interpolate(deck, s, 200 - s.ageMs)).toBeCloseTo(441000 + 0.2 * RATE, 3);
});

test('reverse play wraps to the loop end, never below the start', () => {
  const s = decodeSnapshot(buf, 0);
  const deck = { ...s.decks[0], rate: -1.02 };
  const at = interpolate(deck, s, 100 - s.ageMs);
  expect(at).toBeCloseTo(450000 - (0.1 * RATE - 1000), 3);
  expect(at).toBeGreaterThanOrEqual(440000);
});
