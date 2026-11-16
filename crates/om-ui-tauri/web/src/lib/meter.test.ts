import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { MAX_FALL, MIN_STEP, meterFall, meterFraction, meterZone, moved, type MeterScale } from './meter';

// The table om-ui-bridge's `tests/meter_table.rs` also asserts against `om_ui::meter::MeterScale`.
const table = new TextDecoder()
  .decode(readFileSync(new URL('../../../../../fixtures/ui-bridge/meter-table.txt', import.meta.url)))
  .split('\n')
  .filter((line) => line.trim() !== '' && !line.startsWith('#'))
  .map((line) => line.trim().split(/\s+/));

function tableScale(): MeterScale {
  const row = table.find((r) => r[0] === 'scale');
  if (!row) throw new Error('no scale row');
  return { floorDb: Number(row[1]), midDb: Number(row[2]), highDb: Number(row[3]) };
}

test('every table row maps to its fraction and zone', () => {
  const scale = tableScale();
  const rows = table.filter((r) => r[0] !== 'scale');
  expect(rows.length).toBeGreaterThanOrEqual(6);
  for (const [linear, fraction, zone] of rows) {
    const f = meterFraction(Number(linear), scale.floorDb);
    expect(f, linear).toBeCloseTo(Number(fraction), 5);
    expect(meterZone(f, scale), linear).toBe(zone);
  }
});

test('a scale without headroom below zero shows nothing', () => {
  expect(meterFraction(0.5, 0)).toBe(0);
  expect(meterFraction(Number.NaN, -48)).toBe(0);
});

test('a bar rises at once and falls at most MAX_FALL a frame', () => {
  expect(meterFall(0.2, 0.9)).toBe(0.9);
  expect(meterFall(0.9, 0)).toBeCloseTo(0.9 - MAX_FALL);
  expect(meterFall(0.01, 0)).toBe(0);
});

test('only a move past MIN_STEP is written', () => {
  expect(moved(0.5, 0.5 + MIN_STEP / 2)).toBe(false);
  expect(moved(0.5, 0.5 + MIN_STEP * 2)).toBe(true);
  expect(moved(0.002, 0)).toBe(true);
});
