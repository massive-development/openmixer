import { expect, test } from 'vitest';
import { Scrub } from './scrub';

test('a drag holds the dragged frame and sends it once per drawn frame', () => {
  const s = new Scrub();
  s.begin(5000, 100);
  expect(s.held).toBe(5000);
  expect(s.takeSend()).toBeNull();
  s.move(110, 100, 10_000);
  s.move(120, 100, 10_000);
  expect(s.held).toBe(3000);
  expect(s.takeSend()).toBe(3000);
  expect(s.takeSend()).toBeNull();
});

test('the hold lasts until two snapshots arrive after the last send, once released', () => {
  const s = new Scrub();
  s.begin(5000, 0);
  s.move(10, 100, 10_000);
  s.onSnapshot();
  s.onSnapshot();
  expect(s.held).toBe(4000);
  expect(s.takeSend()).toBe(4000);
  s.onSnapshot();
  s.end();
  expect(s.held).toBe(4000);
  s.onSnapshot();
  expect(s.held).toBeNull();
});

test('a value moved but not yet sent keeps the hold', () => {
  const s = new Scrub();
  s.begin(5000, 0);
  s.move(10, 100, 10_000);
  s.end();
  s.onSnapshot();
  s.onSnapshot();
  expect(s.held).toBe(4000);
  expect(s.takeSend()).toBe(4000);
  s.onSnapshot();
  s.onSnapshot();
  expect(s.held).toBeNull();
});

test('moves outside a drag are ignored', () => {
  const s = new Scrub();
  s.move(10, 100, 10_000);
  expect(s.held).toBeNull();
  expect(s.takeSend()).toBeNull();
});
