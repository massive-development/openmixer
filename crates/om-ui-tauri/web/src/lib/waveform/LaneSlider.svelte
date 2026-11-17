<script lang="ts">
  import { deckName, positionText } from '../a11y';
  import type { LaneSliders } from './sliders.svelte';

  interface Props {
    deck: number;
    sliders: LaneSliders;
  }

  let { deck, sliders }: Props = $props();

  const box = $derived(sliders.boxes[deck]);
  const value = $derived(sliders.values[deck] ?? { seconds: 0, total: 0 });
  const name = $derived(`${deckName(deck)} waveform`);

  // Modified keys belong to the window (and Tab moves on): only a key the lane acts on is taken.
  function onKeyDown(event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    if (sliders.onKey?.(deck, event.key, event.shiftKey)) event.preventDefault();
  }

  // The lanes draw the ring on their own layer, so it shows over the waveform without a stacking order.
  function onFocus(event: FocusEvent): void {
    if ((event.currentTarget as HTMLElement).matches(':focus-visible')) sliders.focused = deck;
  }

  function onBlur(): void {
    if (sliders.focused === deck) sliders.focused = -1;
  }
</script>

{#if box}
  <!-- Laid over the lane, in the tab order after this deck's overview; the pointer passes to the lanes. -->
  <div
    class="lane-slider"
    style:margin-top="{box.top}px"
    style:height="{box.height}px"
    role="slider"
    tabindex="0"
    aria-label={name}
    aria-valuemin={0}
    aria-valuemax={Math.floor(value.total)}
    aria-valuenow={value.seconds}
    aria-valuetext={positionText(value.seconds, value.total)}
    onkeydown={onKeyDown}
    onfocus={onFocus}
    onblur={onBlur}
  ></div>
{/if}

<style>
  .lane-slider {
    grid-area: wave;
    align-self: start;
    pointer-events: none;
  }

  /* The lane under it carries the ring (`focused` in the lanes). */
  .lane-slider:focus-visible {
    outline: none;
  }
</style>
