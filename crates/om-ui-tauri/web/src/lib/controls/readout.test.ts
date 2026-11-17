import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { captionText, type KnobReadout } from './readout';

// The table om-ui-bridge's `tests/knob_text_table.rs` also asserts against `om_ui::control::Readout`.
const rows = new TextDecoder()
  .decode(readFileSync(new URL('../../../../../../fixtures/ui-bridge/knob-text.txt', import.meta.url)))
  .split('\n')
  .filter((line) => line.trim() !== '' && !line.startsWith('#'))
  .map((line) => /^(\S+) (\S+) (\S+) (.+)$/.exec(line));

const READOUTS: readonly string[] = ['gain', 'decibels', 'filter'] satisfies KnobReadout[];

test('the table pins at least twelve values of every knob kind', () => {
  expect(rows.length).toBeGreaterThanOrEqual(12);
  for (const name of READOUTS) expect(rows.some((r) => r?.[2] === name)).toBe(true);
});

test('every table value prints what GPUI prints under the knob', () => {
  for (const row of rows) {
    if (!row) throw new Error('a table row does not parse');
    const [, key, readout, value, text] = row;
    if (!READOUTS.includes(readout)) throw new Error(`${key}: unknown readout ${readout}`);
    expect(captionText(readout as KnobReadout, Number(value)), `${key} ${value}`).toBe(text);
  }
});

test('halves round away from zero, as Rust rounds', () => {
  expect(captionText('decibels', -12.5)).toBe('-13');
  expect(captionText('decibels', 12.5)).toBe('+13');
  expect(captionText('filter', -0.125)).toBe('LPF 13%');
});

test('a value that rounds to zero carries no sign', () => {
  expect(captionText('decibels', -0.4)).toBe('0');
  expect(captionText('gain', 0.99)).toBe('0');
});
