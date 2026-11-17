<script lang="ts">
  import { hotcueName } from '../a11y';
  import Button from '../controls/Button.svelte';
  import { formatTime } from '../format';
  import Icon from '../icons/Icon.svelte';
  import { control, type Controls, type DeckInfo, type StripInk } from '../ipc';
  import { hotcueKeys } from './pads';

  interface Props {
    deck: number;
    controls: Controls;
    info: DeckInfo | null;
    ink: StripInk | null;
    /** An empty deck's pads are disabled. */
    loaded: boolean;
  }

  let { deck, controls, info, ink, loaded }: Props = $props();

  const pads = $derived(hotcueKeys(controls, deck));

  // Seconds into the track of pad `n`'s cue; null while the pad is empty.
  function cueAt(n: number): number | null {
    const frame = info?.hotcues[n] ?? null;
    return frame === null || !info || !(info.sampleRate > 0) ? null : frame / info.sampleRate;
  }

  function send(key: string): void {
    control(key, 1).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }

  // A click sets an empty pad or jumps to a set one; Shift, Alt or Ctrl clears it (GPUI's pads).
  function press(n: number, event: MouseEvent): void {
    const pad = pads[n];
    send(event.shiftKey || event.altKey || event.ctrlKey ? pad.clear : pad.set);
  }

  // Delete or Backspace clears the focused pad, for hands that are on the keys.
  function onKeyDown(n: number, event: KeyboardEvent): void {
    if (event.key !== 'Delete' && event.key !== 'Backspace') return;
    event.preventDefault();
    send(pads[n].clear);
  }

  function hint(label: string, cue: number | null): string {
    if (cue === null) return `Set hot cue ${label} at the playhead`;
    return `Jump to hot cue ${label}. Shift-click or Delete clears it`;
  }
</script>

<div class="pads" role="group" aria-label="Hot cues">
  {#each pads as pad, n (pad.set)}
    {@const cue = cueAt(n)}
    <Button
      on={cue !== null}
      tint={ink?.cues[n] ?? null}
      disabled={!loaded}
      label={hotcueName(deck, pad.label, cue)}
      title={hint(pad.label, cue)}
      control={pad.set}
      onpress={(e) => press(n, e)}
      onkeydown={(e) => onKeyDown(n, e)}
    >
      <span class="pad">
        <!-- The flag marks a pad that holds a cue, so a set pad does not rest on its colour. -->
        <span class="top"><span class="n num">{pad.label}</span>{#if cue !== null}<Icon name="cue" />{/if}</span>
        <span class="t num">{cue === null ? '' : formatTime(cue)}</span>
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
    padding: var(--om-inset-large);
  }

  .pad {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: stretch;
    height: 100%;
    text-align: left;
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
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
