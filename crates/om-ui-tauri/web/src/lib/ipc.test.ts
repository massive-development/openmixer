import { expect, test } from 'vitest';
import { resolveControl } from './ipc';

const ids = { 'deck0.play': 'deck.0.play', 'mixer.crossfader': 'mixer.crossfader' };

test('a known key resolves to its canonical id', () => {
  expect(resolveControl(ids, 'deck0.play')).toBe('deck.0.play');
});

test('an unknown key throws', () => {
  expect(() => resolveControl(ids, 'deck9.play')).toThrow('unknown control key: deck9.play');
  expect(() => resolveControl(ids, 'toString')).toThrow('unknown control key: toString');
});
