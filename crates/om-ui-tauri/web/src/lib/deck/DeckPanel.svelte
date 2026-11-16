<script lang="ts">
  import { interpolate } from '../clock';
  import Fader from '../controls/Fader.svelte';
  import { setText } from '../dom';
  import { formatBpm, formatPitch, formatTime } from '../format';
  import type { Controls, DeckInfo, StripInk } from '../ipc';
  import type { Snapshot } from '../packet';
  import { ticker } from '../ticker';
  import HotcuePads from './HotcuePads.svelte';
  import LoopRow from './LoopRow.svelte';
  import Transport from './Transport.svelte';

  interface Props {
    deck: number;
    controls: Controls;
    info: DeckInfo | null;
    ink: StripInk | null;
    snapshot: Snapshot | null;
    /** Deck B's panel mirrors deck A's: the tempo fader is on each panel's outer edge. */
    mirrored?: boolean;
  }

  let { deck, controls, info, ink, snapshot, mirrored = false }: Props = $props();

  let elapsedEl: HTMLElement;

  const frame = $derived(snapshot?.decks[deck]);
  const loaded = $derived(frame?.loaded ?? false);
  const rate = $derived(frame?.rate ?? 1);
  const bpm = $derived(loaded && info?.bpm ? formatBpm(info.bpm * rate) : '--');
  const pitch = $derived(formatPitch(rate));
  const tempoKey = $derived(`deck${deck}.tempo`);

  $effect(() => {
    if (snapshot) ticker.wake(step);
  });

  // Elapsed time; runs every frame while the deck plays.
  function step(now: number): boolean {
    const snap = snapshot;
    const d = snap?.decks[deck];
    if (!snap || !d || !d.loaded || !(d.fileRate > 0)) {
      setText(elapsedEl, '--');
      return false;
    }
    setText(elapsedEl, formatTime(interpolate(d, snap, now) / d.fileRate));
    return d.playing;
  }
</script>

<div class="panel" class:mirrored>
  <div class="main">
    <div class="readouts">
      <div>
        <span class="label">BPM</span>
        <div class="bpm num">{bpm}</div>
      </div>
      <div class="sub">
        <span class="label">Pitch</span>
        <b class="num">{pitch}</b>
      </div>
      <div class="sub">
        <span class="label">Elapsed</span>
        <b class="num" bind:this={elapsedEl}>--</b>
      </div>
    </div>
    <HotcuePads {deck} {controls} {info} {ink} />
    <LoopRow {deck} {controls} {info} {frame} />
    <Transport {deck} {controls} {frame} />
  </div>
  {#if controls[tempoKey]}
    <div class="tempo">
      <Fader key={tempoKey} spec={controls[tempoKey]} value={rate} stamp={snapshot?.receivedAt ?? 0} />
      <span class="num pitch">{pitch}</span>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: grid;
    grid-template-columns: var(--om-deck-pitch-width) minmax(0, 1fr);
    gap: var(--om-gap);
    min-width: 0;
    min-height: 0;
    padding: var(--om-gap);
    background: var(--om-surface);
  }

  .mirrored {
    grid-template-columns: minmax(0, 1fr) var(--om-deck-pitch-width);
  }

  /* Deck A's fader sits on the left (outer) edge, deck B's on the right. */
  .tempo {
    order: -1;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: var(--om-inner-gap);
    justify-items: center;
    min-height: 0;
  }

  .mirrored .tempo {
    order: 1;
  }

  .main {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto;
    gap: var(--om-gap);
    min-width: 0;
    min-height: 0;
  }

  .readouts {
    display: flex;
    align-items: flex-end;
    gap: var(--om-density-space-5);
    padding-bottom: var(--om-inner-gap);
    border-bottom: var(--om-border-width) solid var(--om-border);
  }

  .label {
    display: block;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
  }

  .bpm {
    color: var(--om-readout);
    font-size: var(--om-type-display);
    font-weight: 400;
    line-height: 1;
  }

  .sub b {
    display: block;
    color: var(--om-readout-dim);
    font-size: var(--om-type-numeral);
    font-weight: 400;
  }

  .pitch {
    color: var(--om-readout-dim);
    font-size: var(--om-type-numeral);
  }
</style>
