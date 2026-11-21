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

/**
 * Pointer drags of tracks and playlists. A press on a source calls `press`; once the pointer has
 * travelled `DRAG_THRESHOLD`, `ghost` follows it with the payload's label and the target under it
 * is marked; a release over a target that accepts the payload calls `ondrop`. The page's other
 * clicks stay clicks: a release after a drag is swallowed by `justDragged`.
 */
export class Drag {
  /** The label that follows the pointer while dragging; null otherwise. */
  ghost: { x: number; y: number; label: string } | null = $state(null);
  private hot: Element | null = null;
  private suppress = false;

  constructor(private readonly ondrop: (payload: Payload, target: Target) => void) {}

  /** True once, right after a drag ended: the click that follows it should be ignored. */
  justDragged(): boolean {
    const was = this.suppress;
    this.suppress = false;
    return was;
  }

  press(event: PointerEvent, payload: Payload): void {
    if (event.button !== 0) return;
    const origin = { x: event.clientX, y: event.clientY };
    let dragging = false;
    const move = (e: PointerEvent): void => {
      if (!dragging && !isDrag(e.clientX - origin.x, e.clientY - origin.y)) return;
      dragging = true;
      this.ghost = { x: e.clientX, y: e.clientY, label: payload.label };
      this.mark(this.under(e, payload)?.element ?? null);
    };
    const end = (e: PointerEvent): void => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      this.mark(null);
      this.ghost = null;
      if (!dragging) return;
      this.suppress = true;
      // A click after a drag fires at once or never; do not leave the flag set for the next press.
      setTimeout(() => (this.suppress = false), 0);
      const hit = e.type === 'pointerup' ? this.under(e, payload) : null;
      if (hit) this.ondrop(payload, hit.target);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
  }

  private under(e: PointerEvent, payload: Payload): { target: Target; element: Element } | null {
    // The ghost is pointer-transparent, so the element under the pointer is the page's own.
    const hit = targetOf(document.elementFromPoint(e.clientX, e.clientY));
    return hit && accepts(payload, hit.target) ? hit : null;
  }

  private mark(element: Element | null): void {
    if (element === this.hot) return;
    this.hot?.removeAttribute(HOT_ATTRIBUTE);
    element?.setAttribute(HOT_ATTRIBUTE, '');
    this.hot = element;
  }
}
