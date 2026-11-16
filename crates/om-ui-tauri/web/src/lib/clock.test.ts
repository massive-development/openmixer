import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { interpolate } from './clock';
import { decodeSnapshot } from './packet';

const golden = readFileSync(new URL('../../../../../fixtures/ui-bridge/snapshot-v1.bin', import.meta.url));
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
