<script lang="ts">
  import { letter } from '../a11y';
  import { interpolate } from '../clock';
  import { setText } from '../dom';
  import { formatBpm, formatTime } from '../format';
  import type { DeckInfo, StripInk } from '../ipc';
  import type { Snapshot } from '../packet';
  import { ticker } from '../ticker';
  import Overview from './Overview.svelte';

  interface Props {
    deck: number;
    info: DeckInfo | null;
    ink: StripInk | null;
    snapshot: Snapshot | null;
  }

  let { deck, info, ink, snapshot }: Props = $props();

  let remainEl: HTMLElement;

  const frame = $derived(snapshot?.decks[deck]);
  const loaded = $derived(frame?.loaded ?? false);
  const key = $derived(loaded ? (info?.keyText ?? '--') : '--');
  const bpm = $derived(loaded && info?.bpm && frame ? formatBpm(info.bpm * frame.rate) : '--');

  $effect(() => {
    if (snapshot) ticker.wake(step);
  });

  // Remaining time at the current rate; runs every frame while the deck plays.
  function step(now: number): boolean {
    const snap = snapshot;
    const d = snap?.decks[deck];
    if (!snap || !d || !d.loaded || !(d.fileRate > 0)) {
      setText(remainEl, '--');
      return false;
    }
    const left = (d.length - interpolate(d, snap, now)) / d.fileRate / (d.rate > 0 ? d.rate : 1);
    setText(remainEl, `-${formatTime(left)}`);
    return d.playing;
  }
</script>

<div class="head" style:--hue="var(--om-accent-{deck})">
  <div class="row">
    <!-- The region's name already says which deck this is. -->
    <div class="letter" aria-hidden="true">{letter(deck)}</div>
    <div class="track">
      <div class="title">{info?.titleText ?? ''}</div>
      <div class="artist">{loaded ? (info?.artist ?? '') : ''}</div>
    </div>
    <div class="stat">
      <span class="label">Key</span>
      <b class="num">{key}</b>
    </div>
    <div class="stat">
      <span class="label">BPM</span>
      <b class="num">{bpm}</b>
    </div>
    <div class="stat time">
      <span class="label">Remain</span>
      <b class="num" bind:this={remainEl}>--</b>
    </div>
  </div>
  <Overview {deck} {info} {ink} {snapshot} />
</div>

<style>
  .head {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: var(--om-inset-large);
    min-width: 0;
    padding: var(--om-inner-gap) var(--om-gap);
    background: var(--om-surface);
  }

  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto auto;
    gap: var(--om-gap);
    align-items: center;
    min-width: 0;
  }

  .letter {
    display: grid;
    place-items: center;
    /* The deck tag's rule at the headline size: the letter's size times the golden ratio. */
    width: calc(var(--om-type-headline) * 1.618);
    height: calc(var(--om-type-headline) * 1.618);
    border-radius: var(--om-control-radius);
    background: var(--hue);
    color: var(--om-on-accent);
    font-family: var(--om-font-display);
    font-size: var(--om-type-headline);
    font-weight: 700;
  }

  .track {
    min-width: 0;
  }

  .title,
  .artist {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title {
    color: var(--om-text-primary);
    font-size: var(--om-type-body);
    font-weight: 500;
  }

  .artist {
    color: var(--om-text-muted);
    font-size: var(--om-type-body);
  }

  .stat {
    text-align: right;
  }

  .label {
    display: block;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
  }

  .stat b {
    display: block;
    color: var(--om-readout);
    font-size: var(--om-type-numeral-large);
    font-weight: 400;
  }

  /* Room for -mm:ss.t, so the column does not shift as minutes change. */
  .time b {
    min-width: 8ch;
  }
</style>
