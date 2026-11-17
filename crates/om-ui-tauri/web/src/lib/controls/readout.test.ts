import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { captionText, spokenText, type KnobReadout } from './readout';

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

test('a gain is spoken in tenths of a decibel, silence as minus infinity', () => {
  expect(spokenText('gain', 1)).toBe('0 dB');
  expect(spokenText('gain', 2)).toBe('plus 6 dB');
  expect(spokenText('gain', 0.5)).toBe('minus 6 dB');
  expect(spokenText('gain', 10 ** (-3.5 / 20))).toBe('minus 3.5 dB');
  expect(spokenText('gain', 0)).toBe('minus infinity');
  expect(spokenText('gain', -0.1)).toBe('minus infinity');
});

test('an EQ band is spoken in decibels, the floor as kill', () => {
  expect(spokenText('decibels', 0)).toBe('0 dB');
  expect(spokenText('decibels', 12)).toBe('plus 12 dB');
  expect(spokenText('decibels', -3.5)).toBe('minus 3.5 dB');
  expect(spokenText('decibels', -0.04)).toBe('0 dB');
  expect(spokenText('decibels', -47.5)).toBe('kill');
  expect(spokenText('decibels', -48)).toBe('kill');
  expect(spokenText('decibels', -47.4)).toBe('minus 47.4 dB');
});

test('the filter is spoken with its side and the word percent', () => {
  expect(spokenText('filter', 0)).toBe('off');
  expect(spokenText('filter', 0.004)).toBe('off');
  expect(spokenText('filter', -0.4)).toBe('low-pass 40 percent');
  expect(spokenText('filter', 1)).toBe('high-pass 100 percent');
  expect(spokenText('filter', -0.125)).toBe('low-pass 13 percent');
});

test('every table value is spoken without a bare hyphen or a percent sign', () => {
  for (const row of rows) {
    if (!row) continue;
    const said = spokenText(row[2] as KnobReadout, Number(row[3]));
    expect(said, `${row[1]} ${row[3]}`).not.toMatch(/(^|\s)[-+−]|%/);
  }
});
