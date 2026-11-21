import { DECK_ATTRIBUTE } from '../loading.svelte';

/** The attribute a drop target carries: `playlist:5`, `folder:3`, `playlists` (the top level) or `row:12`. */
export const DROP_ATTRIBUTE = 'data-drop';
/** Set on the target a drag would drop on, for the highlight. */
export const HOT_ATTRIBUTE = 'data-drop-hot';

/** What is dragged: a library track (from a table row) or a playlist or folder (from the sidebar). */
export type Payload =
  | { kind: 'track'; trackId: number; row: number; label: string }
  | { kind: 'playlist'; id: number; label: string };

/** Where a drop can land. */
export type Target =
  | { kind: 'deck'; deck: number }
  | { kind: 'playlist'; id: number }
  | { kind: 'folder'; id: number | null }
  | { kind: 'row'; row: number };

/** The drop target a `data-drop` value names; null for anything else. */
export function parseDrop(value: string | null): Target | null {
  if (value === null) return null;
  if (value === 'playlists') return { kind: 'folder', id: null };
  const [kind, text] = value.split(':');
  const n = Number(text);
  if (text === undefined || text === '' || !Number.isInteger(n)) return null;
  if (kind === 'playlist') return { kind: 'playlist', id: n };
  if (kind === 'folder') return { kind: 'folder', id: n };
  if (kind === 'row') return { kind: 'row', row: n };
  return null;
}

/** Whether `target` takes `payload`: tracks go to decks, playlists and rows; playlists to folders. */
export function accepts(payload: Payload, target: Target): boolean {
  if (payload.kind === 'track') return target.kind === 'deck' || target.kind === 'playlist' || target.kind === 'row';
  return target.kind === 'folder' && target.id !== payload.id;
}

/** The target under an element: a deck region (`data-deck`) or a `data-drop` element. */
export function targetOf(el: Element | null): { target: Target; element: Element } | null {
  const drop = el?.closest(`[${DROP_ATTRIBUTE}]`);
  if (drop) {
    const target = parseDrop(drop.getAttribute(DROP_ATTRIBUTE));
    if (target) return { target, element: drop };
  }
  const deck = el?.closest(`[${DECK_ATTRIBUTE}]`);
  const n = Number(deck?.getAttribute(DECK_ATTRIBUTE));
  if (deck && Number.isInteger(n)) return { target: { kind: 'deck', deck: n }, element: deck };
  return null;
}

/** The pointer travel before a press becomes a drag, in px. */
export const DRAG_THRESHOLD = 5;

/** Whether a press moved far enough to be a drag. */
export const isDrag = (dx: number, dy: number): boolean => Math.hypot(dx, dy) >= DRAG_THRESHOLD;
