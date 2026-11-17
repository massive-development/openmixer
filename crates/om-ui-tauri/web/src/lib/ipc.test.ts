import { expect, test } from 'vitest';
import { resolveControl, type ControlSpec } from './ipc';

const spec = (id: string): ControlSpec => ({
  id,
  min: 0,
  max: 1,
  default: 0,
  detent: null,
  kind: 'toggle',
  label: '',
  law: { kind: 'linear' },
  ticks: [],
  snap: null,
});
const controls = { 'deck0.play': spec('deck.0.play'), 'mixer.crossfader': spec('mixer.crossfader') };

test('a known key resolves to its spec', () => {
  expect(resolveControl(controls, 'deck0.play').id).toBe('deck.0.play');
});

test('an unknown key throws', () => {
  expect(() => resolveControl(controls, 'deck9.play')).toThrow('unknown control key: deck9.play');
  expect(() => resolveControl(controls, 'toString')).toThrow('unknown control key: toString');
});
