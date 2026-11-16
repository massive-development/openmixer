<script lang="ts">
  import { dbFraction, meterFall, meterFraction, moved, type MeterScale } from '../meter';
  import { ticker } from '../ticker';

  interface Props {
    /** Linear peaks from the snapshot. */
    left: number;
    right: number;
    scale: MeterScale;
  }

  let { left, right, scale }: Props = $props();

  const covers: HTMLElement[] = [];
  const target = new Float64Array(2);
  const shown = new Float64Array(2);
  const written = new Float64Array(2).fill(-1);

  const midAt = $derived(`${dbFraction(scale.midDb, scale.floorDb) * 100}%`);
  const highAt = $derived(`${dbFraction(scale.highDb, scale.floorDb) * 100}%`);

  $effect(() => {
    target[0] = meterFraction(left, scale.floorDb);
    target[1] = meterFraction(right, scale.floorDb);
    ticker.wake(step);
  });

  // One frame of both bars; runs while either is still falling.
  function step(): boolean {
    let falling = false;
    for (let i = 0; i < 2; i++) {
      shown[i] = meterFall(shown[i], target[i]);
      if (shown[i] > target[i]) falling = true;
      if (moved(written[i], shown[i])) write(i);
    }
    return falling;
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

<div class="meter" style:--mid-at={midAt} style:--high-at={highAt}>
  <div class="bar"><i bind:this={covers[0]}></i></div>
  <div class="bar"><i bind:this={covers[1]}></i></div>
</div>

<style>
  .meter {
    display: flex;
    gap: var(--om-inset-small);
    height: 100%;
    min-height: 0;
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
