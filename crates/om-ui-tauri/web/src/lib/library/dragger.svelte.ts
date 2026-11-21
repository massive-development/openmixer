import { accepts, isDrag, targetOf, HOT_ATTRIBUTE, type Payload, type Target } from './drag';

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
