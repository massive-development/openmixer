/** Resolves once the browser has painted at least one frame (the second callback follows a paint). */
export function afterFirstPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}
