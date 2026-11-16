import { deckInfo, onDeckChanged, stripInk, type DeckInfo, type StripInk } from './ipc';

/**
 * What the deck headers and panels show of each deck's track, and each deck's lane ink. Track info
 * is reloaded on `deck-changed` (track, waveform or hot cues); a stale answer is dropped.
 */
export class DeckData {
  infos: (DeckInfo | null)[] = $state.raw([]);
  inks: (StripInk | null)[] = $state.raw([]);
  private requests: number[] = [];

  /** Starts loading `count` decks and follows their changes; resolves with the unlisten call. */
  async start(count: number): Promise<() => void> {
    this.infos = Array.from({ length: count }, () => null);
    this.inks = Array.from({ length: count }, () => null);
    this.requests = Array.from({ length: count }, () => 0);
    for (let deck = 0; deck < count; deck++) {
      void this.load(deck);
      void this.loadInk(deck);
    }
    return onDeckChanged(({ deck }) => {
      if (deck < count) void this.load(deck);
    });
  }

  private async load(deck: number): Promise<void> {
    const request = ++this.requests[deck];
    try {
      const info = await deckInfo(deck);
      if (request === this.requests[deck]) this.infos = replaced(this.infos, deck, info);
    } catch (error) {
      console.error(`openmixer: deck ${deck} info`, error);
    }
  }

  private async loadInk(deck: number): Promise<void> {
    try {
      const ink = await stripInk(deck);
      this.inks = replaced(this.inks, deck, ink);
    } catch (error) {
      console.error(`openmixer: deck ${deck} ink`, error);
    }
  }
}

// A copy with one entry changed: the arrays are raw state, replaced whole so readers see the change.
function replaced<T>(items: readonly T[], at: number, value: T): T[] {
  const next = items.slice();
  next[at] = value;
  return next;
}
