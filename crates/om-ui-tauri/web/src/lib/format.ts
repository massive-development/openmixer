/** `mm:ss.t`, e.g. `04:17.3`; a negative time formats its absolute value. Tenths are cut, as a clock counts. */
export function formatTime(seconds: number): string {
  const tenths = Number.isFinite(seconds) ? Math.floor(Math.abs(seconds) * 10 + 1e-6) : 0;
  const minutes = Math.floor(tenths / 600);
  const rest = tenths - minutes * 600;
  const secs = Math.floor(rest / 10);
  return `${pad2(minutes)}:${pad2(secs)}.${rest % 10}`;
}

/** Two decimals, e.g. `140.00`. */
export function formatBpm(bpm: number): string {
  return bpm.toFixed(2);
}

/** The rate as a signed percentage with two decimals: `+2.00%`, `-0.50%`, `+0.00%`. */
export function formatPitch(rate: number): string {
  const percent = (rate - 1) * 100;
  const text = Math.abs(percent).toFixed(2);
  // A rate that rounds to zero reads `+0.00%`, never `-0.00%`.
  const sign = percent < 0 && Number(text) !== 0 ? '-' : '+';
  return `${sign}${text}%`;
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}
