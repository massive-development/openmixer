import { expect, test } from 'vitest';
import { HudWindow, hudText } from './hud';

test('the HUD reads drawn frames a second and the mean work a frame', () => {
  expect(hudText(30, 15, 500)).toBe('60 fps · 0.50 ms');
  expect(hudText(0, 0, 500)).toBe('0 fps · 0.00 ms');
  expect(hudText(5, 1, 0)).toBe('0 fps · 0.20 ms');
});

test('the counter reads out once per period, over the drawn frames only', () => {
  const hud = new HudWindow(500);
  expect(hud.frame(0, true, 2)).toBeNull();
  expect(hud.frame(100, false, 9)).toBeNull();
  expect(hud.frame(250, true, 4)).toBeNull();
  // Three frames counted from 0: two drawn (2 ms and 4 ms of work) over 500 ms.
  expect(hud.frame(500, false, 1)).toBe('4 fps · 3.00 ms');
  expect(hud.frame(600, true, 1)).toBeNull();
  expect(hud.frame(1000, false, 0)).toBe('2 fps · 1.00 ms');
});
