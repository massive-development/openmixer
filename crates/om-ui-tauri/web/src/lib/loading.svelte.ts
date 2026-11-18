import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { loadFile, onLoadFailed, type LoadFailed } from './ipc';

/** The attribute a deck's regions carry, so a drop lands on the deck under the pointer. */
export const DECK_ATTRIBUTE = 'data-deck';

/** How long a deck shows why its load failed. */
const NOTE_MS = 6000;

/** The file types the open dialog offers: the formats the library reads. */
export const AUDIO_EXTENSIONS = ['mp3', 'wav', 'flac', 'aiff', 'aif', 'm4a', 'aac', 'ogg', 'opus'];

/** A deck's region on the page: a header, a panel or a waveform lane. */
export interface DeckRegion {
  deck: number;
  rect: { left: number; top: number; right: number; bottom: number };
}

/**
 * The deck a drop at (`x`, `y`) lands on: the deck region holding the point, else the half of the
 * window it is in (deck A on the left). Regions naming a deck outside `count` are ignored.
 */
export function deckAt(regions: readonly DeckRegion[], x: number, y: number, width: number, count: number): number {
  const inside = regions.find(
    ({ deck, rect }) =>
      deck >= 0 && deck < count && x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom,
  );
  if (inside) return inside.deck;
  return x < width / 2 ? 0 : Math.min(1, count - 1);
}

/** The deck regions on the page now, from their `data-deck` attribute. */
function deckRegions(): DeckRegion[] {
  return Array.from(document.querySelectorAll(`[${DECK_ATTRIBUTE}]`), (el) => ({
    deck: Number(el.getAttribute(DECK_ATTRIBUTE)),
    rect: el.getBoundingClientRect(),
  })).filter((r) => Number.isInteger(r.deck));
}

/** What a deck says after a failed load: the file, then why. */
export function noteText(failure: LoadFailed): string {
  return `${failure.file}: ${failure.reason}`;
}

/**
 * Loading files onto decks: the open dialog, files dropped from the desktop (the deck under the
 * pointer is marked while a drag is over the window), and the notes that say why a load failed.
 */
export class Loading {
  /** The deck a drag over the window would drop onto; null while nothing is dragged. */
  target: number | null = $state(null);
  notes: (string | null)[] = $state([]);
  private timers: ReturnType<typeof setTimeout>[] = [];

  /** Follows drags and failed loads for `count` decks; resolves with the unlisten call. */
  async start(count: number): Promise<() => void> {
    this.notes = Array.from({ length: count }, () => null);
    const failed = await onLoadFailed((failure) => this.note(failure.deck, noteText(failure)));
    const dropped = await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === 'leave') {
        this.target = null;
        return;
      }
      const ratio = window.devicePixelRatio || 1;
      const x = payload.position.x / ratio;
      const y = payload.position.y / ratio;
      const deck = deckAt(deckRegions(), x, y, window.innerWidth, count);
      if (payload.type === 'drop') {
        this.target = null;
        const first = payload.paths[0];
        if (first) this.load(deck, first);
      } else this.target = deck;
    });
    return () => {
      failed();
      dropped();
    };
  }

  /** Asks for one audio file and loads it into `deck`. */
  async choose(deck: number): Promise<void> {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Audio', extensions: AUDIO_EXTENSIONS }],
    });
    if (typeof path === 'string') this.load(deck, path);
  }

  private load(deck: number, path: string): void {
    this.note(deck, null);
    loadFile(deck, path).catch((error: unknown) => this.note(deck, String(error)));
  }

  private note(deck: number, text: string | null): void {
    if (deck < 0 || deck >= this.notes.length) return;
    clearTimeout(this.timers[deck]);
    this.notes[deck] = text;
    if (text !== null) this.timers[deck] = setTimeout(() => (this.notes[deck] = null), NOTE_MS);
  }
}
