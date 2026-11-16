import { expect, test } from 'vitest';
import type { StripInk } from '../ipc';
import { hexRgb, laneInk, levelRows, packRows } from './gl';
import { TEXTURE_WIDTH } from './shader';

const ink: StripInk = {
  well: '#000000',
  body: '#e3a24f',
  highs: '#ffffff',
  highsAlpha: 0.55,
  playedShade: '#0a0a09',
  playedShadeAlpha: 0.6,
  gridBar: '#ffffff',
  gridBarAlpha: 0.14,
  gridBeat: '#ffffff',
  gridBeatAlpha: 0.05,
  playhead: '#ff0000',
  playheadHalo: null,
  playheadHaloWidth: 1,
  playheadWidth: 2,
  cueGuide: '#e3a24f',
  cueGuideAlpha: 0.45,
  mainCue: '#ff9c78',
  cues: [],
  barNumberSize: 12,
  barNumberMinSpacingPx: 48,
  cueFlagWidth: 18,
  cueFlagHeight: 16,
};

test('hexRgb reads #rrggbb into 0..1 channels', () => {
  const out = hexRgb('#e3a24f', new Float32Array(3));
  expect(out[0]).toBeCloseTo(0xe3 / 255);
  expect(out[1]).toBeCloseTo(0xa2 / 255);
  expect(out[2]).toBeCloseTo(0x4f / 255);
  expect(Array.from(hexRgb('red', new Float32Array(3)))).toEqual([0, 0, 0]);
});

test('laneInk keeps the alphas and turns a missing shade or halo into zero width', () => {
  const lane = laneInk(ink);
  expect(lane.gridBar[3]).toBeCloseTo(0.14);
  expect(lane.gridBeat[3]).toBeCloseTo(0.05);
  expect(lane.shadeAlpha).toBeCloseTo(0.6);
  expect(lane.haloWidth).toBe(0);
  expect(laneInk({ ...ink, playedShade: null }).shadeAlpha).toBe(0);
});

test('levels start on their own rows', () => {
  const { rows, total } = levelRows([
    { samplesPerPoint: 256, len: TEXTURE_WIDTH + 1 },
    { samplesPerPoint: 1024, len: 3 },
  ]);
  expect(Array.from(rows)).toEqual([0, 2]);
  expect(total).toBe(3);
  expect(levelRows([]).total).toBe(0);
});

test('packRows places each level at its row and refuses bytes of another layout', () => {
  const levels = [
    { samplesPerPoint: 256, len: 2 },
    { samplesPerPoint: 1024, len: 1 },
  ];
  const { rows, total } = levelRows(levels);
  const bytes = Uint8Array.from([1, 2, 3, 4, 5, 6, 7, 8, 9]);
  const out = packRows(levels, bytes, rows, total);
  expect(out?.length).toBe(TEXTURE_WIDTH * 2 * 3);
  expect(Array.from(out?.subarray(0, 6) ?? [])).toEqual([1, 2, 3, 4, 5, 6]);
  const second = TEXTURE_WIDTH * 3;
  expect(Array.from(out?.subarray(second, second + 3) ?? [])).toEqual([7, 8, 9]);
  expect(packRows(levels, bytes.subarray(0, 8), rows, total)).toBeNull();
  expect(packRows(levels, new Uint8Array(12), rows, total)).toBeNull();
});
