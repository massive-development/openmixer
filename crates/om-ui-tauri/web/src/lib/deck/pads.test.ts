import { expect, test } from 'vitest';
import { hotcueKeys, loopButtons, loopBeats } from './pads';

const spec = {
  id: '',
  min: 0,
  max: 1,
  default: 0,
  detent: null,
  kind: 'trigger' as const,
  label: '',
  law: { kind: 'linear' as const },
  ticks: [],
  snap: null,
  writes: [],
};
const controls = {
  'deck0.loop.4': { ...spec, label: '4' },
  'deck0.loop.0.25': { ...spec, label: '1/4' },
  'deck0.loop.exit': spec,
  'deck0.hotcue0': spec,
  'deck0.hotcue0.clear': spec,
  'deck0.hotcue1': spec,
  'deck1.loop.1': spec,
};

test('loop buttons are the deck loop sizes the shell lists, shortest first', () => {
  expect(loopButtons(controls, 0)).toEqual([
    { key: 'deck0.loop.0.25', beats: 0.25, label: '1/4' },
    { key: 'deck0.loop.4', beats: 4, label: '4' },
  ]);
  expect(loopButtons(controls, 2)).toEqual([]);
});

test('hot cue pads are the indices with both a set and a clear id', () => {
  expect(hotcueKeys(controls, 0)).toEqual([{ set: 'deck0.hotcue0', clear: 'deck0.hotcue0.clear', label: '' }]);
});

test('a loop length in beats on a constant grid', () => {
  expect(loopBeats(0, 44100, 120, 44100)).toBeCloseTo(2);
  expect(loopBeats(0, 0, 120, 44100)).toBe(0);
  expect(loopBeats(0, 44100, null, 44100)).toBe(0);
  expect(loopBeats(0, 44100, 120, 0)).toBe(0);
});
