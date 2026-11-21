import { headerState, type HeaderState } from '../ipc';

/** How often the header is read: the elapsed time moves once a second, the peaks tenths of one. */
export const HEADER_PERIOD_MS = 100;

/**
 * The top bar's Automix strip and Record button state. It polls the shell while the window is
 * visible; a run with no automix or recorder rejects the first read and the header stays hidden.
 */
export class Header {
  /** The last read; null until the first one, and for a run with no automix or recorder. */
  state: HeaderState | null = $state.raw(null);
  /** A press that failed, shown for a moment next to the bar. */
  failure: string | null = $state(null);
  private timer: ReturnType<typeof setInterval> | undefined;
  private reading = false;

  /** Starts polling; returns what stops it. */
  start(read: () => Promise<HeaderState> = headerState, period = HEADER_PERIOD_MS): () => void {
    const poll = async (): Promise<void> => {
      // A slow read is not queued behind itself.
      if (this.reading || document.hidden) return;
      this.reading = true;
      try {
        this.state = await read();
      } catch {
        this.state = null;
        this.stop();
      } finally {
        this.reading = false;
      }
    };
    void poll();
    this.timer = setInterval(() => void poll(), period);
    return () => this.stop();
  }

  private stop(): void {
    clearInterval(this.timer);
    this.timer = undefined;
  }

  /** Runs a press, then reads again at once so the bar does not wait for the next poll. */
  async press(action: () => Promise<void>, read: () => Promise<HeaderState> = headerState): Promise<void> {
    try {
      await action();
      this.failure = null;
      this.state = await read();
    } catch (error) {
      this.failure = String(error);
      console.error('openmixer: header press', error);
    }
  }
}
