/** Most a bar falls in one frame, as a share of its height. */
export const MAX_FALL = 0.03;
/** Least change of a bar worth writing to the page. */
export const MIN_STEP = 0.003;

/** A bar's dB scale, from the theme (`--om-meter-floor-db`, `--om-meter-mid-db`, `--om-meter-high-db`). */
export interface MeterScale {
  floorDb: number;
  midDb: number;
  highDb: number;
}

export type MeterZone = 'low' | 'mid' | 'high';

/** Where `db` sits on a bar over `floorDb..0 dB`, `0..1` (`om_ui::meter::MeterScale::proportion`). */
export function dbFraction(db: number, floorDb: number): number {
  if (!(floorDb < 0)) return 0;
  const f = (db - floorDb) / -floorDb;
  return Number.isNaN(f) ? 0 : Math.min(Math.max(f, 0), 1);
}

/** A linear peak's height on the bar; silence and junk are 0. */
export function meterFraction(linear: number, floorDb: number): number {
  return linear > 0 ? dbFraction(20 * Math.log10(linear), floorDb) : 0;
}

/**
 * The level a meter reports to assistive technology: the louder side in whole decibels, held to
 * `floorDb..0`. Whole steps keep a moving level from changing the report on every packet.
 */
export function peakDb(left: number, right: number, floorDb: number): number {
  const peak = Math.max(left > 0 ? left : 0, right > 0 ? right : 0);
  if (!(peak > 0)) return floorDb;
  return Math.min(Math.max(Math.round(20 * Math.log10(peak)), floorDb), 0);
}

/** The colour band a bar height falls in (`om_ui::meter::MeterScale::zone`). */
export function meterZone(fraction: number, scale: MeterScale): MeterZone {
  const db = scale.floorDb + Math.min(Math.max(fraction, 0), 1) * -scale.floorDb;
  if (db >= scale.highDb) return 'high';
  return db >= scale.midDb ? 'mid' : 'low';
}

/** The next shown height: it jumps up to `target` and falls towards it by at most `MAX_FALL`. */
export function meterFall(shown: number, target: number): number {
  return Math.max(target, shown - MAX_FALL);
}

/** Whether the bar moved far enough from what was written to write it again. */
export function moved(written: number, shown: number): boolean {
  return Math.abs(shown - written) > MIN_STEP || (shown === 0 && written !== 0);
}
