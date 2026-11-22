import { expect, test } from 'vitest';
import { paramFraction, paramValue, usesLsb } from './devices';
import { Pager } from './pager.svelte';

const view = <P>(page: P, notice: { text: string; error: boolean } | null = null) => ({ page, notice });

test('a parameter slider maps its range to 0..1 and back, clamped', () => {
  const p = { min: -2, max: 2, value: 1 };
  expect(paramFraction(p)).toBe(0.75);
  expect(paramValue(p, 0.75)).toBe(1);
  expect(paramValue(p, 7)).toBe(2);
  expect(paramFraction({ min: 0, max: 0, value: 0 })).toBe(0);
});

test('only the 14-bit mode shows the LSB field', () => {
  expect(usesLsb('absolute14')).toBe(true);
  expect(usesLsb('absolute')).toBe(false);
});

test('calls run in order and a poll waits for an edit in flight', async () => {
  const seen: number[] = [];
  let release: (() => void) | undefined;
  const slow = new Promise<void>((resolve) => (release = resolve));
  const pager = new Pager(async () => view(0));
  const edit = pager.send(async () => {
    await slow;
    seen.push(1);
    return view(1);
  });
  await pager.poll(); // skipped: an edit is on its way
  release?.();
  await edit;
  expect(seen).toEqual([1]);
  expect(pager.page).toBe(1);
  await pager.poll();
  expect(pager.page).toBe(0);
});

test('a notice from a poll shows, and an edit without one clears it', async () => {
  const pager = new Pager(async () => view(0, { text: 'Learned a control', error: false }));
  await pager.poll();
  expect(pager.notice?.text).toBe('Learned a control');
  await pager.send(async () => view(1));
  expect(pager.notice).toBeNull();
});

test('a failed call shows as an error and keeps the page', async () => {
  const pager = new Pager(async () => view(5));
  await pager.poll();
  await pager.send(async () => {
    throw 'map not loaded';
  });
  expect(pager.page).toBe(5);
  expect(pager.notice).toEqual({ text: 'map not loaded', error: true });
});
