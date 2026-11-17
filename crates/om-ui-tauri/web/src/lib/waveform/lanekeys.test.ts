import { expect, test } from 'vitest';
import { laneKey, type LaneKeyState } from './lanekeys';
import { DEFAULT_VISIBLE_SECONDS, zoomBy } from './view';

const RATE = 48_000;
// 120 BPM: a beat is half a second.
const BEAT = RATE / 2;
const gridded: LaneKeyState = {
  frame: 10 * RATE,
  length: 60 * RATE,
  sampleRate: RATE,
  bpm: 120,
  firstBeatFrame: 1_000,
  seconds: 8,
};
const plain: LaneKeyState = { ...gridded, bpm: null, firstBeatFrame: null };

test('arrows move one beat on a grid, four with Shift', () => {
  expect(laneKey('ArrowRight', false, gridded)).toEqual({ kind: 'seek', frame: 10 * RATE + BEAT });
  expect(laneKey('ArrowLeft', false, gridded)).toEqual({ kind: 'seek', frame: 10 * RATE - BEAT });
  expect(laneKey('ArrowRight', true, gridded)).toEqual({ kind: 'seek', frame: 10 * RATE + 4 * BEAT });
  expect(laneKey('ArrowLeft', true, gridded)).toEqual({ kind: 'seek', frame: 10 * RATE - 4 * BEAT });
});

test('without a grid the arrows move half a second, two with Shift', () => {
  expect(laneKey('ArrowRight', false, plain)).toEqual({ kind: 'seek', frame: 10.5 * RATE });
  expect(laneKey('ArrowLeft', true, plain)).toEqual({ kind: 'seek', frame: 8 * RATE });
  // A tempo without beat 0 is no grid.
  expect(laneKey('ArrowRight', false, { ...gridded, firstBeatFrame: null })).toEqual({ kind: 'seek', frame: 10.5 * RATE });
  expect(laneKey('ArrowRight', false, { ...gridded, bpm: 0 })).toEqual({ kind: 'seek', frame: 10.5 * RATE });
});

test('a seek stays inside the track, as the scrub does', () => {
  expect(laneKey('ArrowLeft', true, { ...plain, frame: RATE })).toEqual({ kind: 'seek', frame: 0 });
  expect(laneKey('ArrowRight', true, { ...plain, frame: 59.5 * RATE })).toEqual({ kind: 'seek', frame: 60 * RATE - 1 });
  expect(laneKey('Home', false, gridded)).toEqual({ kind: 'seek', frame: 0 });
  expect(laneKey('End', false, gridded)).toEqual({ kind: 'seek', frame: 60 * RATE - 1 });
});

test('without a track nothing seeks', () => {
  const empty = { ...plain, length: 0, frame: 0 };
  for (const key of ['ArrowLeft', 'ArrowRight', 'Home', 'End']) expect(laneKey(key, false, empty)).toBeNull();
  expect(laneKey('ArrowRight', false, { ...plain, sampleRate: 0 })).toBeNull();
});

test('plus and minus zoom by one wheel notch, inside 1 to 64 s; 0 resets', () => {
  expect(zoomBy(8, -1)).toBeCloseTo(8 / 1.15);
  expect(laneKey('+', true, gridded)).toEqual({ kind: 'zoom', seconds: zoomBy(8, -1) });
  expect(laneKey('=', false, gridded)).toEqual({ kind: 'zoom', seconds: zoomBy(8, -1) });
  expect(laneKey('-', false, gridded)).toEqual({ kind: 'zoom', seconds: zoomBy(8, 1) });
  expect(laneKey('+', false, { ...gridded, seconds: 1 })).toEqual({ kind: 'zoom', seconds: 1 });
  expect(laneKey('-', false, { ...gridded, seconds: 64 })).toEqual({ kind: 'zoom', seconds: 64 });
  expect(laneKey('0', false, { ...gridded, seconds: 30 })).toEqual({ kind: 'zoom', seconds: DEFAULT_VISIBLE_SECONDS });
  expect(DEFAULT_VISIBLE_SECONDS).toBe(8);
  // Zoom needs no track.
  expect(laneKey('-', false, { ...plain, length: 0 })).toEqual({ kind: 'zoom', seconds: zoomBy(8, 1) });
});

test('every other key is left alone, so Tab moves on', () => {
  for (const key of ['Tab', 'Enter', ' ', 'ArrowUp', 'ArrowDown', 'PageUp', 'a', '1']) {
    expect(laneKey(key, false, gridded)).toBeNull();
  }
});
