import { describe, expect, it } from 'vitest';
import type { Pad } from '../ipc';
import { filterText, gainText, padForKey, pitchText } from './pads';

const pad = (n: number): Pad => ({
  pad: n,
  key: String(n + 1),
  name: 'Empty',
  loaded: false,
  missing: false,
  loading: false,
  playing: false,
  gain: 1,
  pitch: 0,
  mode: 'one-shot',
  selected: false,
});

describe('padForKey', () => {
  const pads = [pad(0), pad(1), pad(7)];
  it('finds the pad that reports the key', () => {
    expect(padForKey(pads, '2')).toBe(1);
    expect(padForKey(pads, '8')).toBe(7);
  });
  it('is null for another key', () => {
    expect(padForKey(pads, 'a')).toBeNull();
    expect(padForKey(pads, '9')).toBeNull();
  });
});

describe('read-outs', () => {
  it('prints gain as a percentage', () => {
    expect(gainText(0.9)).toBe('90%');
    expect(gainText(1.5)).toBe('150%');
  });
  it('prints pitch with its sign', () => {
    expect(pitchText(0)).toBe('+0.0 st');
    expect(pitchText(-2.5)).toBe('-2.5 st');
  });
  it('names the filter side and depth', () => {
    expect(filterText(0)).toBe('Off');
    expect(filterText(-0.5)).toBe('Low-pass 50%');
    expect(filterText(1)).toBe('High-pass 100%');
  });
});
