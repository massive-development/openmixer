<script lang="ts">
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
  const playing = $derived(frame?.playing ?? false);
  const synced = $derived(frame?.sync ?? false);

  // A toggle sends the opposite of what the engine reports.
  function toggle(name: string, on: boolean): void {
    const key = `deck${deck}.${name}`;
    control(key, on ? 0 : 1).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }
</script>

<div class="transport">
  {#if play}
    <Button tall on={playing} tint="var(--om-accent-{deck})" onpress={() => toggle('play', playing)}>
      <!-- Play reads Pause while the deck plays, as GPUI's `TransportPanel::play_label`. -->
      {playing ? 'Pause' : play.label}
    </Button>
  {/if}
  {#if sync}
    <Button tall on={synced} onBorder="var(--om-sync-on)" onpress={() => toggle('sync', synced)}>
      {sync.label}
    </Button>
  {/if}
</div>

<style>
  .transport {
    display: grid;
    grid-template-columns: 1.6fr 1fr;
    gap: var(--om-inner-gap);
  }
</style>
