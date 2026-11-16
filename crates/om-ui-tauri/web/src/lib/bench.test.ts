import { expect, test } from 'vitest';
import { benchStats, FrameRecorder, summarise } from './bench';

test('summarise', () => {
  const s = summarise([1, 2, 3, 4, 100]);
  expect(s).toMatchObject({ n: 5, mean: 22, p50: 3, max: 100 });
  expect(summarise([])).toEqual({ n: 0, mean: 0, p50: 0, p95: 0, p99: 0, max: 0 });
});

test('summarise one sample gives that sample everywhere', () => {
  expect(summarise([7])).toEqual({ n: 1, mean: 7, p50: 7, p95: 7, p99: 7, max: 7 });
});

test('summarise sorts its input and leaves the caller list alone', () => {
  const samples = [100, 4, 1, 3, 2];
  expect(summarise(samples)).toEqual({ n: 5, mean: 22, p50: 3, p95: 100, p99: 100, max: 100 });
  expect(samples).toEqual([100, 4, 1, 3, 2]);
});

test('the recorder keeps frames up to its capacity, then drops them', () => {
  const rec = new FrameRecorder(1);
  expect(rec.capacity).toBe(240);
  for (let i = 0; i < 250; i++) rec.push(16, 0.5);
  expect(rec.length).toBe(240);
  expect(rec.dropped).toBe(10);
});

test('bench stats count slow frames against twice the median interval', () => {
  const rec = new FrameRecorder(1);
  for (const ms of [10, 10, 10, 25, 20]) rec.push(ms, 1);
  const stats = benchStats(rec, { benchSeconds: 1, fpsCap: 60, viewport: [800, 600, 1], decksPlaying: 2 });
  expect(stats.framesOver2xRefresh).toBe(1);
  expect(stats.fps).toBeCloseTo(1000 / 15);
  expect(stats.frameMs.n).toBe(5);
  expect(stats.workMs.mean).toBe(1);
  expect(stats.decksPlaying).toBe(2);
});

test('bench stats with no frames report no fps', () => {
  const stats = benchStats(new FrameRecorder(1), { benchSeconds: 1, fpsCap: 0, viewport: [1, 1, 1], decksPlaying: 0 });
  expect(stats.fps).toBeNull();
  expect(stats.framesOver2xRefresh).toBe(0);
});
