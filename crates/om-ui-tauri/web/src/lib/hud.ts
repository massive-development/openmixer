/** `NN fps · N.NN ms`: frames drawn per second over `elapsedMs`, and the mean work per drawn frame. */
export function hudText(frames: number, workMs: number, elapsedMs: number): string {
  const fps = elapsedMs > 0 ? (frames * 1000) / elapsedMs : 0;
  const mean = frames > 0 ? workMs / frames : 0;
  return `${fps.toFixed(0)} fps · ${mean.toFixed(2)} ms`;
}
