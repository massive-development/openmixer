/** Writes `text` into `el` only when it differs, so unchanged read-outs cost no layout. */
export function setText(el: HTMLElement | undefined, text: string): void {
  if (el && el.textContent !== text) el.textContent = text;
}

/** Sets one inline style property only when its value changes. */
export function setStyle(el: HTMLElement | undefined, name: string, value: string): void {
  if (el && el.style.getPropertyValue(name) !== value) el.style.setProperty(name, value);
}

/** A theme number such as `--om-meter-floor-db`, read from the root's computed style; NaN when absent. */
export function themeNumber(name: string): number {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
}
