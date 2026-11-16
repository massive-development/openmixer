import { expect, test } from 'vitest';
import { hudText } from './hud';

test('the HUD reads drawn frames a second and the mean work a frame', () => {
  expect(hudText(30, 15, 500)).toBe('60 fps · 0.50 ms');
  expect(hudText(0, 0, 500)).toBe('0 fps · 0.00 ms');
  expect(hudText(5, 1, 0)).toBe('0 fps · 0.20 ms');
});
