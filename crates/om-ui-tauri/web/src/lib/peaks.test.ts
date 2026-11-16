import { expect, test } from 'vitest';
import { decodePeaks, needsPeaks, PEAKS_HEADER_LEN, PeakSync } from './peaks';

test('needsPeaks only when the peaks generation moved', () => {
  expect(needsPeaks(null, 0)).toBe(true);
  expect(needsPeaks(3, 3)).toBe(false);
  expect(needsPeaks(3, 4)).toBe(true);
});

test('decodePeaks reads the generation before the bytes', () => {
  const buf = new ArrayBuffer(PEAKS_HEADER_LEN + 4);
  new DataView(buf).setBigUint64(0, 5n, true);
  new Uint8Array(buf).set([1, 2, 3, 4], PEAKS_HEADER_LEN);
  const peaks = decodePeaks(buf);
  expect(peaks?.generation).toBe(5);
  expect(Array.from(peaks?.bytes ?? [])).toEqual([1, 2, 3, 4]);
  expect(decodePeaks(new ArrayBuffer(PEAKS_HEADER_LEN))?.bytes.length).toBe(0);
  expect(decodePeaks(new ArrayBuffer(3))).toBeNull();
});

test('a hot cue change (same peaks generation) fetches nothing', () => {
  const sync = new PeakSync();
  expect(sync.onInfo(1, true)).toBe(true);
  expect(sync.onPeaks(1, 1)).toBe(true);
  expect(sync.onInfo(1, true)).toBe(false);
  expect(sync.onInfo(1, true)).toBe(false);
  expect(sync.shown).toBe(1);
});

test('a second info for the generation being fetched does not fetch it twice', () => {
  const sync = new PeakSync();
  expect(sync.onInfo(1, true)).toBe(true);
  expect(sync.onInfo(1, true)).toBe(false);
});

test('a new waveform fetches once and marks the shown peaks as older', () => {
  const sync = new PeakSync();
  sync.onInfo(1, true);
  sync.onPeaks(1, 1);
  expect(sync.onInfo(2, true)).toBe(true);
  expect(sync.showsOlder()).toBe(true);
  expect(sync.onPeaks(2, 2)).toBe(true);
  expect(sync.showsOlder()).toBe(false);
});

test('a late answer for an older generation never wins', () => {
  const sync = new PeakSync();
  expect(sync.onInfo(1, true)).toBe(true);
  expect(sync.onInfo(2, true)).toBe(true);
  expect(sync.onPeaks(2, 2)).toBe(true);
  expect(sync.onPeaks(1, 1)).toBe(false);
  expect(sync.shown).toBe(2);
});

test('peaks newer than the info are not shown against the older levels', () => {
  const sync = new PeakSync();
  sync.onInfo(1, true);
  expect(sync.onPeaks(1, 2)).toBe(false);
  expect(sync.onInfo(2, true)).toBe(true);
});

test('an info older than the newest is ignored', () => {
  const sync = new PeakSync();
  sync.onInfo(3, true);
  expect(sync.onInfo(2, true)).toBe(false);
  expect(sync.onPeaks(2, 2)).toBe(false);
});

test('a deck without peaks fetches nothing and shows that generation empty', () => {
  const sync = new PeakSync();
  expect(sync.onInfo(4, false)).toBe(false);
  expect(sync.shown).toBe(4);
  expect(sync.showsOlder()).toBe(false);
});

test('a failed fetch is retried on the next info', () => {
  const sync = new PeakSync();
  expect(sync.onInfo(1, true)).toBe(true);
  sync.onFailed(1);
  expect(sync.onInfo(1, true)).toBe(true);
});
