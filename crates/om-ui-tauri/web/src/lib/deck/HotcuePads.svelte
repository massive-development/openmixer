<script lang="ts">
  import Button from '../controls/Button.svelte';
  import { formatTime } from '../format';
  import { control, type Controls, type DeckInfo, type StripInk } from '../ipc';
  import { hotcueKeys } from './pads';

  interface Props {
    deck: number;
    controls: Controls;
    info: DeckInfo | null;
    ink: StripInk | null;
  }

  let { deck, controls, info, ink }: Props = $props();

  const pads = $derived(hotcueKeys(controls, deck));

  function cueAt(n: number): number | null {
    return info?.hotcues[n] ?? null;
  }

  function time(frame: number): string {
    return info && info.sampleRate > 0 ? formatTime(frame / info.sampleRate) : '';
  }

  // A click sets an empty pad or jumps to a set one; Shift, Alt or Ctrl clears it (GPUI's pads).
  function press(n: number, event: MouseEvent): void {
    const pad = pads[n];
    const key = event.shiftKey || event.altKey || event.ctrlKey ? pad.clear : pad.set;
    control(key, 1).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }
</script>

<div class="pads">
  {#each pads as pad, n (pad.set)}
    {@const cue = cueAt(n)}
    <Button on={cue !== null} tint={ink?.cues[n] ?? null} onpress={(e) => press(n, e)}>
      <span class="pad">
        <span class="n num">{pad.label}</span>
        <span class="t num">{cue === null ? '' : time(cue)}</span>
      </span>
    </Button>
  {/each}
</div>

<style>
  .pads {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    grid-auto-rows: minmax(var(--om-min-target), 1fr);
    gap: var(--om-inner-gap);
    min-height: 0;
  }

  .pads :global(.btn) {
    height: 100%;
    padding: var(--om-inset-large) var(--om-inner-gap);
  }

  .pad {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: flex-start;
    height: 100%;
  }

  .n {
    font-size: var(--om-type-numeral);
    font-weight: 700;
  }

  .t {
    color: var(--om-text-secondary);
    font-size: var(--om-type-numeral);
  }
</style>
