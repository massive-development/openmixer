<script lang="ts">
  import { meterText, Paced } from '../a11y';
  import { dbFraction, meterFall, meterFraction, moved, peakDb, type MeterScale } from '../meter';
  import { ticker } from '../ticker';

  interface Props {
    /** Linear peaks from the snapshot. */
    left: number;
    right: number;
    scale: MeterScale;
    /** What the meter measures, for assistive technology: `Channel A level`. */
    label: string;
  }

  // The level reaches assistive technology at most four times a second, never per frame.
  const SPOKEN_PERIOD_MS = 250;

  let { left, right, scale, label }: Props = $props();

  const covers: HTMLElement[] = [];
  const target = new Float64Array(2);
  const shown = new Float64Array(2);
  const written = new Float64Array(2).fill(-1);
  const paced = new Paced(SPOKEN_PERIOD_MS);
  // The level last reported, and the one waiting for its turn.
  let spoken: number | undefined = $state();
  let latest: number | undefined;

  const midAt = $derived(`${dbFraction(scale.midDb, scale.floorDb) * 100}%`);
  const highAt = $derived(`${dbFraction(scale.highDb, scale.floorDb) * 100}%`);

  $effect(() => {
    target[0] = meterFraction(left, scale.floorDb);
    target[1] = meterFraction(right, scale.floorDb);
    latest = Number.isFinite(scale.floorDb) ? peakDb(left, right, scale.floorDb) : undefined;
    ticker.wake(step);
  });

  // One frame of both bars; runs while either is still falling or a level waits to be reported.
  function step(now: number): boolean {
    let falling = false;
    for (let i = 0; i < 2; i++) {
      shown[i] = meterFall(shown[i], target[i]);
      if (shown[i] > target[i]) falling = true;
      if (moved(written[i], shown[i])) write(i);
    }
    return report(now) || falling;
  }

  // Hands the latest level to assistive technology when its turn comes; true while it waits.
  function report(now: number): boolean {
    if (latest === spoken) return false;
    if (!paced.due(now)) return true;
    spoken = latest;
    return false;
  }

  // The well-coloured cover hides the bar above the level: one transform per bar.
  function write(i: number): void {
    const el = covers[i];
    if (!el) return;
    written[i] = shown[i];
    el.style.transform = `scaleY(${1 - shown[i]})`;
    ticker.drew();
  }
</script>

<div
  class="meter"
  role="meter"
  aria-label={label}
  aria-valuemin={spoken === undefined ? undefined : scale.floorDb}
  aria-valuemax={spoken === undefined ? undefined : 0}
  aria-valuenow={spoken}
  aria-valuetext={spoken === undefined ? undefined : meterText(spoken, scale.floorDb)}
  style:--mid-at={midAt}
  style:--high-at={highAt}
>
  <div class="bar"><i bind:this={covers[0]}></i></div>
  <div class="bar"><i bind:this={covers[1]}></i></div>
</div>

<style>
  .meter {
    position: relative;
    display: flex;
    gap: var(--om-inset-small);
    height: 100%;
    min-height: 0;
  }

  /* A mark at each threshold, past both sides of the bars, so the zones read without their colours. */
  .meter::before {
    content: '';
    position: absolute;
    inset: 0 calc(-1 * var(--om-inset-large));
    background:
      linear-gradient(var(--om-border-strong), var(--om-border-strong)) 0 calc(100% - var(--mid-at)) / 100%
        var(--om-border-width) no-repeat,
      linear-gradient(var(--om-border-strong), var(--om-border-strong)) 0 calc(100% - var(--high-at)) / 100%
        var(--om-border-width) no-repeat;
  }

  /* The three zones split at the theme's thresholds, under a segmented mask and the cover. */
  .bar {
    position: relative;
    width: var(--om-mixer-meter-width);
    height: 100%;
    overflow: hidden;
    border-radius: var(--om-bar-radius);
    background: linear-gradient(
      to top,
      var(--om-meter-low) 0 var(--mid-at),
      var(--om-meter-mid) var(--mid-at) var(--high-at),
      var(--om-meter-high) var(--high-at)
    );
  }

  .bar i {
    position: absolute;
    inset: 0;
    background: var(--om-well);
    transform-origin: top;
    will-change: transform;
  }

  .bar::after {
    content: '';
    position: absolute;
    inset: 0;
    background: repeating-linear-gradient(
      to top,
      transparent 0 var(--om-meter-segment),
      var(--om-surface) var(--om-meter-segment) calc(var(--om-meter-segment) + var(--om-meter-segment-gap))
    );
  }
</style>
