/**
 * A knob's value as GPUI's mixer prints it under the label: `om_ui::control::Readout::text` without
 * the unit the label names (`om_ui_bridge::caption_text`). The control spec's `readout` says which
 * format a knob uses; `fixtures/ui-bridge/knob-text.txt` pins this port to the Rust one.
 */
export type KnobReadout = 'gain' | 'decibels' | 'filter';

// `om_ui::format::EQ_KILL_DB` and its half-decibel margin: at or under this a band reads KILL.
const KILL_AT_DB = -48 + 0.5;
// `om_ui::format::filter`: closer to the centre than this reads as off.
const FILTER_OFF = 0.005;

/** What GPUI prints under the knob's label for `value`, in engine units. */
export function captionText(readout: KnobReadout, value: number): string {
  if (readout === 'gain') return value > 0 ? wholeDb(20 * Math.log10(value)) : '-inf';
  if (readout === 'decibels') return value <= KILL_AT_DB ? 'KILL' : wholeDb(value);
  return filterText(value);
}

// `format::whole_decibels` without its unit: a sign on positives, never `-0`.
function wholeDb(db: number): string {
  const whole = roundHalfAway(db);
  if (whole === 0) return '0';
  return whole > 0 ? `+${whole}` : String(whole);
}

function filterText(value: number): string {
  if (Math.abs(value) < FILTER_OFF) return 'Off';
  return `${value < 0 ? 'LPF' : 'HPF'} ${roundHalfAway(Math.abs(value) * 100)}%`;
}

/**
 * What a screen reader speaks for a knob at `value`: the caption's reading with its unit and its
 * sign in words (`0 dB`, `plus 12 dB`, `minus 3.5 dB`, `kill`, `off`, `low-pass 40 percent`,
 * `minus infinity`), to a tenth of a decibel where the caption rounds to whole ones.
 */
export function spokenText(readout: KnobReadout, value: number): string {
  if (readout === 'gain') return value > 0 ? spokenDb(20 * Math.log10(value)) : 'minus infinity';
  if (readout === 'decibels') return value <= KILL_AT_DB ? 'kill' : spokenDb(value);
  return spokenFilter(value);
}

// Tenths of a decibel, the sign as a word; a value that rounds to zero carries none.
function spokenDb(db: number): string {
  const tenths = roundHalfAway(db * 10) / 10;
  if (tenths === 0) return '0 dB';
  return `${tenths > 0 ? 'plus' : 'minus'} ${Math.abs(tenths)} dB`;
}

function spokenFilter(value: number): string {
  if (Math.abs(value) < FILTER_OFF) return 'off';
  return `${value < 0 ? 'low-pass' : 'high-pass'} ${roundHalfAway(Math.abs(value) * 100)} percent`;
}

// Rust's `f64::round`: halves go away from zero, where `Math.round` takes them up.
function roundHalfAway(value: number): number {
  return Math.sign(value) * Math.round(Math.abs(value));
}
