<script lang="ts">
  import { controlName } from '../a11y';
  import Button from '../controls/Button.svelte';
  import Icon from '../icons/Icon.svelte';
  import { control, type Controls } from '../ipc';
  import type { DeckFrame } from '../packet';
  import { cueWrites, sendInOrder } from './transport';

  interface Props {
    deck: number;
    controls: Controls;
    frame: DeckFrame | undefined;
    /** Deck B's row mirrors deck A's: Sync, Play, Cue. */
    mirrored?: boolean;
  }

  let { deck, controls, frame, mirrored = false }: Props = $props();

  const cue = $derived(controls[`deck${deck}.cue`]);
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

  // GPUI's Cue: each write waits for the one before, so the engine stops before it seeks.
  function pressCue(): void {
    sendInOrder(cueWrites(controls, deck), control).catch((error: unknown) =>
      console.error(`openmixer: deck${deck}.cue`, error),
    );
  }
</script>

<!-- Cue, Play, Sync in the source on both decks; deck B's row is mirrored by grid areas. -->
<div class="transport" class:mirrored>
  {#if cue}
    <!-- A press, not a toggle: it never stays lit. -->
    <div class="slot cue">
      <Button tall tint={hue} disabled={!loaded} label={controlName(`deck${deck}.cue`, cue.label)} control="deck{deck}.cue" onpress={pressCue}>
        <span class="face">{cue.label}</span>
      </Button>
    </div>
  {/if}
  {#if play}
    <!-- Its word and glyph change with the state, so it carries no pressed state of its own. -->
    <div class="slot play">
      <Button
        tall
        solid
        on={playing}
        tint={hue}
        disabled={!loaded}
        label={controlName(`deck${deck}.play`, playWord)}
        control="deck{deck}.play"
        onpress={() => toggle('play', playing)}
      >
        <span class="face"><Icon name={playing ? 'pause' : 'play'} />{playWord}</span>
      </Button>
    </div>
  {/if}
  {#if sync}
    <div class="slot sync">
      <Button
        tall
        on={synced}
        pressed={synced}
        tint={hue}
        disabled={!loaded}
        label={controlName(`deck${deck}.sync`, sync.label)}
        control="deck{deck}.sync"
        onpress={() => toggle('sync', synced)}
      >
        <span class="face">
          <Icon name="sync" />{sync.label}
          <!-- The state in a word, as GPUI's LOCK beside Sync. -->
          {#if synced}<b class="word num">Lock</b>{/if}
        </span>
      </Button>
    </div>
  {/if}
</div>

<style>
  .transport {
    display: grid;
    /* Cue, Play (the wider track), Sync: Play and Cue on the outer side, as in GPUI. */
    grid-template-columns: 1fr 1.6fr 1fr;
    grid-template-areas: 'cue play sync';
    gap: var(--om-inner-gap);
  }

  .mirrored {
    grid-template-areas: 'sync play cue';
  }

  /* A grid of one, so the button stretches over its slot as it did over the row's track. */
  .slot {
    display: grid;
  }

  .cue {
    grid-area: cue;
  }

  .play {
    grid-area: play;
  }

  .sync {
    grid-area: sync;
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
