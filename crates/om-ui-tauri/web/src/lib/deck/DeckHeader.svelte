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
    /** Deck B's header mirrors deck A's: read-outs from the centre, the letter at the outer edge. */
    mirrored?: boolean;
  }

  let { deck, info, ink, snapshot, mirrored = false }: Props = $props();

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

<!-- Mirrored by grid areas only, so both decks keep one source, reading and Tab order. -->
<div class="head" style:--hue="var(--om-accent-{deck})">
  <div class="row" class:mirrored>
    <!-- The region's name already says which deck this is. -->
    <div class="letter" aria-hidden="true">{letter(deck)}</div>
    <div class="track">
      <div class="title" class:empty={!loaded}>{info?.titleText ?? ''}</div>
      <div class="artist">{loaded ? (info?.artist ?? '') : ''}</div>
    </div>
    <div class="stat key">
      <span class="label">Key</span>
      <b class="num">{key}</b>
    </div>
    <div class="stat bpm">
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
    grid-template-areas: 'letter track key bpm time';
    gap: var(--om-gap);
    align-items: center;
    min-width: 0;
  }

  /* The read-outs start from the centre line and the track's words end at the letter. */
  .row.mirrored {
    grid-template-columns: auto auto auto minmax(0, 1fr) auto;
    grid-template-areas: 'time bpm key track letter';
  }

  .letter {
    grid-area: letter;
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
    grid-area: track;
    min-width: 0;
  }

  .mirrored .track {
    text-align: right;
  }

  .key {
    grid-area: key;
  }

  .bpm {
    grid-area: bpm;
  }

  .time {
    grid-area: time;
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

  /* An empty deck's words step back. */
  .empty {
    color: var(--om-text-muted);
    font-weight: 400;
  }

  .artist {
    color: var(--om-text-muted);
    font-size: var(--om-type-body);
  }

  .stat {
    text-align: right;
  }

  .mirrored .stat {
    text-align: left;
  }

  /* The label and numeral tiers' own line heights, so the pair keeps inside the row at 125% text. */
  .label {
    display: block;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    line-height: 1.2;
    text-transform: uppercase;
  }

  .stat b {
    display: block;
    color: var(--om-readout);
    margin-top: var(--om-inset-small);
    font-size: var(--om-type-numeral-large);
    font-weight: 400;
    line-height: 1;
  }

  /* Room for -mm:ss.t, so the column does not shift as minutes change. */
  .time b {
    min-width: 8ch;
  }
</style>
