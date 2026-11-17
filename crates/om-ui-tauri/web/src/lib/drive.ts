/**
 * `--ui-drive-out`: the map an outside driver (`tools/ui-drive.py`) reads to find each control on
 * screen. Every element with a `data-control` key is measured once the layout settles, and again
 * after a resize; nothing here runs on the frame loop, and nothing runs without the flag.
 */

/** A box in CSS px relative to the window's content area. */
export interface ControlRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** What the shell's `ui_layout` takes (`om_ui_tauri::LayoutMap`). */
export interface LayoutMap {
  devicePixelRatio: number;
  viewport: { width: number; height: number };
  controls: Record<string, ControlRect>;
}

/** The parts of an element the map reads. */
export interface Placed {
  getAttribute(name: string): string | null;
  getBoundingClientRect(): { left: number; top: number; width: number; height: number };
}

/** The parts of the window the map reads. */
export interface View {
  innerWidth: number;
  innerHeight: number;
  devicePixelRatio: number;
}

/** The attribute that names an element's control key. */
export const CONTROL_ATTRIBUTE = 'data-control';

/** The map of `elements` by their control key; the first element with a key wins, one without is skipped. */
export function layoutMap(elements: Iterable<Placed>, view: View): LayoutMap {
  const controls: Record<string, ControlRect> = {};
  for (const element of elements) {
    const key = element.getAttribute(CONTROL_ATTRIBUTE);
    if (!key || Object.hasOwn(controls, key)) continue;
    const r = element.getBoundingClientRect();
    controls[key] = { x: r.left, y: r.top, width: r.width, height: r.height };
  }
  return {
    devicePixelRatio: view.devicePixelRatio || 1,
    viewport: { width: view.innerWidth, height: view.innerHeight },
    controls,
  };
}
