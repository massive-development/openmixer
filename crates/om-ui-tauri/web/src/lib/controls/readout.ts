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

// Rust's `f64::round`: halves go away from zero, where `Math.round` takes them up.
function roundHalfAway(value: number): number {
  return Math.sign(value) * Math.round(Math.abs(value));
}
