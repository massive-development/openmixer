/**
 * When a deck's peaks are fetched and shown. `deck-changed` fires for hot cues and titles too; the
 * peaks (hundreds of KB, one GPU upload) are fetched only when the deck's `peaksGeneration` moves,
 * and an answer for an older generation never replaces a newer one.
 */

/** Bytes before the peaks in a `deck_peaks` answer: the peaks generation, u64 little-endian. */
export const PEAKS_HEADER_LEN = 8;

/** A deck's packed peaks and the generation they were read at. */
export interface Peaks {
  generation: number;
  bytes: Uint8Array;
}

/** A `deck_peaks` answer; null when it is too short to hold its generation. */
export function decodePeaks(buf: ArrayBuffer): Peaks | null {
  if (buf.byteLength < PEAKS_HEADER_LEN) return null;
  const generation = Number(new DataView(buf).getBigUint64(0, true));
  return { generation, bytes: new Uint8Array(buf, PEAKS_HEADER_LEN) };
}

/** Whether peaks of generation `next` must be fetched when `prev` is shown (null: none yet). */
export function needsPeaks(prev: number | null, next: number): boolean {
  return prev !== next;
}

/** One deck's peaks: the generation shown, the newest one the deck info named, the one in flight. */
export class PeakSync {
  /** The generation whose peaks are shown (empty for a deck without peaks); null before any. */
  shown: number | null = null;
  private latest: number | null = null;
  private fetching: number | null = null;

  /** Deck info named `generation`; true when its peaks must be fetched now. */
  onInfo(generation: number, hasPeaks: boolean): boolean {
    if (this.latest !== null && generation < this.latest) return false;
    this.latest = generation;
    if (!needsPeaks(this.shown, generation) || this.fetching === generation) return false;
    if (!hasPeaks) {
      this.shown = generation;
      return false;
    }
    this.fetching = generation;
    return true;
  }

  /** A fetch asked at `requested` returned peaks of `received`; true when they are to be shown. */
  onPeaks(requested: number, received: number): boolean {
    if (this.fetching === requested) this.fetching = null;
    if (received !== this.latest) return false;
    this.shown = received;
    return true;
  }

  /** The fetch for `requested` failed; the next info asks again. */
  onFailed(requested: number): void {
    if (this.fetching === requested) this.fetching = null;
  }

  /** Whether the peaks on show belong to an older waveform than the newest info. */
  showsOlder(): boolean {
    return this.shown !== null && this.latest !== null && this.shown !== this.latest;
  }
}
