import { failure, type Notice, type View } from './devices';

/**
 * One settings page's state: the page as the shell last sent it and the notice it gave. Calls go
 * one at a time, in order, so a poll never overwrites what a later edit changed; a poll is skipped
 * while an edit is still on its way.
 */
export class Pager<P> {
  page: P | null = $state(null);
  notice: Notice | null = $state(null);
  busy = $state(false);
  private pending = 0;
  private tail: Promise<void> = Promise.resolve();
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private readonly read: () => Promise<View<P>>,
    /** Called with each page that arrives, before it is shown. */
    private readonly onPage: (page: P, edit: boolean) => void = () => {},
  ) {}

  /** Runs one edit (or read) after the calls before it; a failed call shows as an error notice. */
  send(call: () => Promise<View<P>>, edit = true): Promise<void> {
    this.pending += 1;
    this.busy = true;
    const run = async (): Promise<void> => {
      try {
        const view = await call();
        this.onPage(view.page, edit);
        this.page = view.page;
        if (view.notice) this.notice = view.notice;
        else if (edit) this.notice = null;
      } catch (error) {
        this.notice = failure(error);
      } finally {
        this.pending -= 1;
        this.busy = this.pending > 0;
      }
    };
    this.tail = this.tail.then(run);
    return this.tail;
  }

  /** Reads the page again; skipped while an edit is on its way. */
  poll(): Promise<void> {
    return this.pending > 0 ? Promise.resolve() : this.send(this.read, false);
  }

  /** Reads now, then every `ms` until [`stop`](Pager.stop). */
  start(ms: number): void {
    void this.poll();
    this.timer = setInterval(() => void this.poll(), ms);
  }

  stop(): void {
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }
}
