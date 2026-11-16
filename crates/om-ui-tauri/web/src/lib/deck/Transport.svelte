<script lang="ts">
  import { controlName } from '../a11y';
  import Button from '../controls/Button.svelte';
  import { control, type Controls } from '../ipc';
  import type { DeckFrame } from '../packet';

  interface Props {
    deck: number;
    controls: Controls;
    frame: DeckFrame | undefined;
  }

  let { deck, controls, frame }: Props = $props();

  const play = $derived(controls[`deck${deck}.play`]);
  const sync = $derived(controls[`deck${deck}.sync`]);
  const loaded = $derived(frame?.loaded ?? false);
  const playing = $derived(frame?.playing ?? false);
  const synced = $derived(frame?.sync ?? false);
  const hue = $derived(`var(--om-accent-${deck})`);
  // Play reads Pause while the deck plays, as GPUI's `TransportPanel::play_label`.
  const playWord = $derived(playing ? 'Pause' : (play?.label ?? ''));

  // A toggle sends the opposite of what the engine reports.
  function toggle(name: string, on: boolean): void {
    const key = `deck${deck}.${name}`;
    control(key, on ? 0 : 1).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }
</script>

<div class="transport">
  {#if play}
    <!-- Its word changes with the state, so it carries no pressed state of its own. -->
    <Button
      tall
      solid
      on={playing}
      tint={hue}
      disabled={!loaded}
      label={controlName(`deck${deck}.play`, playWord)}
      onpress={() => toggle('play', playing)}
    >
      <span class="face">{playWord}</span>
    </Button>
  {/if}
  {#if sync}
    <Button
      tall
      on={synced}
      pressed={synced}
      tint={hue}
      disabled={!loaded}
      label={controlName(`deck${deck}.sync`, sync.label)}
      onpress={() => toggle('sync', synced)}
    >
      <span class="face">
        {sync.label}
        <!-- The state in a word, as GPUI's LOCK beside Sync. -->
        {#if synced}<b class="word num">Lock</b>{/if}
      </span>
    </Button>
  {/if}
</div>

<style>
  .transport {
    display: grid;
    grid-template-columns: 1.6fr 1fr;
    gap: var(--om-inner-gap);
  }

  .face {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--om-density-space-3);
    white-space: nowrap;
  }

  .word {
    font-size: var(--om-type-label);
    font-weight: 700;
  }
</style>
