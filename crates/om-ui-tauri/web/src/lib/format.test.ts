import { expect, test } from 'vitest';
import { formatBpm, formatPitch, formatTime } from './format';

test('formatTime', () => {
  expect(formatTime(257.34)).toBe('04:17.3');
  expect(formatTime(0)).toBe('00:00.0');
  expect(formatTime(-12.5)).toBe('00:12.5');
  // Tenths are cut, as a clock counts, so 59.96 s never reads as 60 seconds.
  expect(formatTime(59.96)).toBe('00:59.9');
  expect(formatTime(Number.NaN)).toBe('00:00.0');
});

test('formatBpm and formatPitch', () => {
  expect(formatBpm(140)).toBe('140.00');
  expect(formatPitch(1.02)).toBe('+2.00%');
  expect(formatPitch(0.995)).toBe('-0.50%');
  expect(formatPitch(1)).toBe('+0.00%');
  // A rate a hair under 1 rounds to zero and keeps the plus sign.
  expect(formatPitch(0.99999)).toBe('+0.00%');
});
