import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: vi.fn() }));

const { deckAt, noteText } = await import('./loading.svelte');

const region = (deck: number, left: number, top: number, right: number, bottom: number) => ({
  deck,
  rect: { left, top, right, bottom },
});

// Headers side by side on top, two full-width lanes stacked below them.
const page = [
  region(0, 0, 0, 800, 100),
  region(1, 800, 0, 1600, 100),
  region(0, 0, 100, 1600, 200),
  region(1, 0, 200, 1600, 300),
];

describe('deckAt', () => {
  it('takes the deck region holding the point', () => {
    expect(deckAt(page, 100, 50, 1600, 2)).toBe(0);
    expect(deckAt(page, 900, 50, 1600, 2)).toBe(1);
  });

  it('follows the stacked lanes, not the side of the window', () => {
    expect(deckAt(page, 1500, 150, 1600, 2)).toBe(0);
    expect(deckAt(page, 100, 250, 1600, 2)).toBe(1);
  });

  it('falls back to the half of the window outside every region', () => {
    expect(deckAt(page, 799, 500, 1600, 2)).toBe(0);
    expect(deckAt(page, 800, 500, 1600, 2)).toBe(1);
  });

  it('ignores a region naming a deck that does not exist', () => {
    expect(deckAt([region(7, 0, 0, 1600, 900)], 1500, 10, 1600, 2)).toBe(1);
  });
});

describe('noteText', () => {
  it('names the file, then the reason', () => {
    expect(noteText({ deck: 0, file: 'intro.flac', reason: 'deck is playing' })).toBe('intro.flac: deck is playing');
  });
});
