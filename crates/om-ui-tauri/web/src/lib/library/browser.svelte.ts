import { libraryView, type LibraryView, type RowWindow } from './ipc';

/** How often the browser asks again while it is open: analysis finishes and scans land on their own. */
export const POLL_MS = 400;

/** The rows asked for before the table has measured itself. */
const FIRST_WINDOW: RowWindow = { first: 0, count: 30 };

/**
 * The browser's state: the last view the shell answered, and the window of rows it was asked for.
 * Every action sends the window and applies the answer; an answer older than the newest request
 * is dropped, so a slow reply never overwrites a newer one.
 */
export class LibraryBrowser {
  view: LibraryView | null = $state.raw(null);
  /** Why there is no view (no library is open), in words. */
  problem: string | null = $state(null);
  private window: RowWindow = FIRST_WINDOW;
  private latest = 0;

  /** The rows to ask for now. */
  get rows(): RowWindow {
    return this.window;
  }

  /** The table scrolled or resized: ask for these rows. */
  show(window: RowWindow): void {
    if (window.first === this.window.first && window.count === this.window.count) return;
    this.window = window;
    void this.refresh();
  }

  /** Asks again; resolves once the answer is applied. */
  refresh(): Promise<void> {
    return this.run((w) => libraryView(w));
  }

  /** Runs one action with the current window and applies its answer. */
  async run(call: (window: RowWindow) => Promise<LibraryView>): Promise<void> {
    const request = ++this.latest;
    try {
      const view = await call(this.window);
      if (request === this.latest) this.apply(view);
    } catch (error) {
      if (request === this.latest) this.problem = String(error);
    }
  }

  /** Like `run`, for an action whose answer carries more than the view. */
  async runWith<T>(call: (window: RowWindow) => Promise<[T, LibraryView]>): Promise<T | null> {
    const request = ++this.latest;
    try {
      const [extra, view] = await call(this.window);
      if (request === this.latest) this.apply(view);
      return extra;
    } catch (error) {
      if (request === this.latest) this.problem = String(error);
      return null;
    }
  }

  /** Follows the library until the returned call is made. */
  start(): () => void {
    void this.refresh();
    const timer = setInterval(() => void this.refresh(), POLL_MS);
    return () => clearInterval(timer);
  }

  private apply(view: LibraryView): void {
    this.problem = null;
    this.view = view;
  }
}
