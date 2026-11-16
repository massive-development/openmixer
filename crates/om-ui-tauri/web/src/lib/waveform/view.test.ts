import { expect, test } from 'vitest';
import {
  barStep,
  firstLabelledBar,
  frameToX,
  laneRect,
  pickLevel,
  scrubFrame,
  seekValue,
  shouldDraw,
  viewScale,
  zoomBy,
} from './view';

const levels = [
  { samplesPerPoint: 256, len: 44000 },
  { samplesPerPoint: 1024, len: 11000 },
  { samplesPerPoint: 4096, len: 2750 },
];

test('pickLevel chooses the coarsest level that still resolves a pixel', () => {
  expect(pickLevel(levels, 100)).toBe(0);
  expect(pickLevel(levels, 256)).toBe(0);
  expect(pickLevel(levels, 2000)).toBe(1);
  expect(pickLevel(levels, 50000)).toBe(2);
});
test('pickLevel on an empty list', () => {
  expect(pickLevel([], 500)).toBe(-1);
});

test('viewScale', () => {
  expect(viewScale(8, 44100, 1600)).toBeCloseTo(220.5);
});
test('viewScale_zero_width', () => {
  expect(viewScale(8, 44100, 0)).toBe(0);
  expect(viewScale(8, 44100, -4)).toBe(0);
});
test('viewScale without a sample rate', () => {
  expect(viewScale(8, 0, 1600)).toBe(0);
});

// The plan's cases, at the plan's 46 px spacing.
test('barStep', () => {
  expect(barStep(120, 46)).toBe(1);
  expect(barStep(46, 46)).toBe(1);
  expect(barStep(30, 46)).toBe(2);
  expect(barStep(5, 46)).toBe(16);
});

// om_ui::waveform::bar_label_step's own tests, at BAR_NUMBER_MIN_SPACING_PX (48).
test('barStep matches bar_label_step', () => {
  expect(barStep(200, 48)).toBe(1);
  expect(barStep(48, 48)).toBe(1);
  expect(barStep(40, 48)).toBe(2);
  expect(barStep(20, 48)).toBe(4);
  expect(barStep(11, 48)).toBe(8);
  for (const px of [3, 9, 17, 31, 47, 90]) {
    expect(barStep(px, 48) * px >= 48 || px >= 48).toBe(true);
  }
});
test('barStep stays finite for a zero or negative bar', () => {
  expect(barStep(0, 48)).toBe(1 << 20);
  expect(barStep(-40, 48)).toBe(2);
});

test('frameToX puts the playhead frame at the centre', () => {
  expect(frameToX(1000, 1000, 10, 800)).toBe(400);
  expect(frameToX(1100, 1000, 10, 800)).toBe(410);
  expect(frameToX(900, 1000, 10, 800)).toBe(390);
});

test('firstLabelledBar starts at a multiple of the step, never before bar 0', () => {
  // A bar is 1000 frames from frame 500.
  expect(firstLabelledBar(-10_000, 500, 1000, 1)).toBe(0);
  expect(firstLabelledBar(3600, 500, 1000, 1)).toBe(4);
  expect(firstLabelledBar(3600, 500, 1000, 4)).toBe(4);
  expect(firstLabelledBar(5600, 500, 1000, 4)).toBe(8);
});

test('zoomBy steps by 1.15 per notch inside 1..64 s', () => {
  expect(zoomBy(8, 100)).toBeCloseTo(9.2);
  expect(zoomBy(8, -100)).toBeCloseTo(8 / 1.15);
  expect(zoomBy(8, 0)).toBe(8);
  expect(zoomBy(63, 100)).toBe(64);
  expect(zoomBy(1.05, -100)).toBe(1);
});

test('shouldDraw caps the frame rate, with 2 ms of slack', () => {
  expect(shouldDraw(1000, 1000 - 14.7, 60)).toBe(true);
  expect(shouldDraw(1000, 1000 - 10, 60)).toBe(false);
  expect(shouldDraw(1000, 999, 0)).toBe(true);
});

test('seekValue is the fraction of the track', () => {
  expect(seekValue(500, 1000)).toBe(0.5);
  expect(seekValue(-5, 1000)).toBe(0);
  expect(seekValue(2000, 1000)).toBe(1);
  expect(seekValue(10, 0)).toBe(0);
});

test('scrubFrame moves the track against the drag and stays inside it', () => {
  // Dragging right by 10 device px at 100 frames per px pulls earlier audio under the playhead.
  expect(scrubFrame(5000, 10, 100, 10_000)).toBe(4000);
  expect(scrubFrame(5000, -10, 100, 10_000)).toBe(6000);
  expect(scrubFrame(500, 10, 100, 10_000)).toBe(0);
  expect(scrubFrame(9500, -10, 100, 10_000)).toBe(9999);
});

test('laneRect splits the height evenly with separators between lanes', () => {
  expect(laneRect(0, 2, 201, 1)).toEqual({ top: 0, height: 100 });
  expect(laneRect(1, 2, 201, 1)).toEqual({ top: 101, height: 100 });
  expect(laneRect(0, 1, 120, 1)).toEqual({ top: 0, height: 120 });
  expect(laneRect(0, 2, 1, 1)).toEqual({ top: 0, height: 0 });
  expect(laneRect(0, 0, 100, 1)).toEqual({ top: 0, height: 0 });
});
