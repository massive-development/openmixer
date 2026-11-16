<script lang="ts">
  import { loopName } from '../a11y';
  import Button from '../controls/Button.svelte';
  import { control, type Controls, type DeckInfo } from '../ipc';
  import type { DeckFrame } from '../packet';
  import { loopBeats, loopButtons } from './pads';

  interface Props {
    deck: number;
    controls: Controls;
    info: DeckInfo | null;
    frame: DeckFrame | undefined;
  }

  // GPUI's `loops::MATCH_TOLERANCE`: a loop snapped to the grid is within rounding of its size.
  const MATCH_BEATS = 0.01;

  let { deck, controls, info, frame }: Props = $props();

  const sizes = $derived(loopButtons(controls, deck));
  const exitKey = $derived(`deck${deck}.loop.exit`);
  const loaded = $derived(frame?.loaded ?? false);
  const beats = $derived(
    frame?.looping ? loopBeats(frame.loopStart, frame.loopEnd, info?.bpm ?? null, frame.fileRate) : 0,
  );

  function lit(size: number): boolean {
    return beats > 0 && Math.abs(beats - size) < MATCH_BEATS;
  }

  // A lit size ends its loop; any other starts a loop of that size.
  function press(key: string, size: number): void {
    const send = lit(size) && Object.hasOwn(controls, exitKey) ? exitKey : key;
    control(send, 1).catch((error: unknown) => console.error(`openmixer: ${send}`, error));
  }
</script>

{#if sizes.length > 0}
  <div class="loops" role="group" aria-label="Loop">
    <span class="label" aria-hidden="true">Loop</span>
    {#each sizes as size (size.key)}
      <Button
        on={lit(size.beats)}
        pressed={lit(size.beats)}
        tint="var(--om-accent-{deck})"
        disabled={!loaded}
        label={loopName(deck, size.beats, size.label)}
        onpress={() => press(size.key, size.beats)}
      >
        <span class="num">{size.label}</span>
      </Button>
    {/each}
  </div>
{/if}

<style>
  .loops {
    display: grid;
    grid-auto-columns: minmax(0, 1fr);
    grid-auto-flow: column;
    grid-template-columns: auto;
    gap: var(--om-inset-large);
    align-items: center;
  }

  /* Too narrow for eight pointer targets in a row (1280 px windows): two rows of four. */
  @container (max-width: 360px) {
    .loops {
      grid-auto-flow: row;
      grid-template-columns: auto repeat(4, minmax(0, 1fr));
    }

    .label {
      grid-row: span 2;
    }
  }

  .label {
    display: inline-flex;
    align-items: center;
    gap: var(--om-density-space-3);
    padding-right: var(--om-inner-gap);
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
  }

  .num {
    font-size: var(--om-type-numeral);
    text-transform: none;
  }

  .loops :global(.btn) {
    min-width: 0;
    padding: 0;
  }
</style>
