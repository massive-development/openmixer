<script lang="ts">
  import { interpolate } from '../clock';
  import Fader from '../controls/Fader.svelte';
  import { setText } from '../dom';
  import { formatBpm, formatPitch, formatTime } from '../format';
  import type { Controls, DeckInfo, StemsState, StripInk } from '../ipc';
  import type { Snapshot } from '../packet';
  import { ticker } from '../ticker';
  import DeckTag from './DeckTag.svelte';
  import HotcuePads from './HotcuePads.svelte';
  import LoopRow from './LoopRow.svelte';
  import StemsPanel from './StemsPanel.svelte';
  import Transport from './Transport.svelte';

  interface Props {
    deck: number;
    controls: Controls;
    info: DeckInfo | null;
    ink: StripInk | null;
    snapshot: Snapshot | null;
    /** The deck's stems; null until the shell reports them. */
    stems?: StemsState | null;
    /** The drawer opens on stems (a staged scene). */
    startOnStems?: boolean;
    /** Deck B's panel mirrors deck A's: the tempo fader is on each panel's outer edge. */
    mirrored?: boolean;
  }

  let { deck, controls, info, ink, snapshot, stems = null, startOnStems = false, mirrored = false }: Props = $props();

  // The drawer under the read-outs: the hot cue pads or the stems.
  let tab: 'cues' | 'stems' = $state('cues');
  $effect(() => {
    if (startOnStems) tab = 'stems';
  });

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

<!-- Both decks share one source order (tempo, read-outs, pads, loops, transport), so Tab and a
     screen reader take them alike; deck B is mirrored by grid areas and flex direction only. -->
<div class="panel" class:mirrored>
  {#if controls[tempoKey]}
    <div class="tempo">
      <!-- As in GPUI: an accidental double-click mid-mix must not jump the tempo. -->
      <!-- The pitch shows once, in the read-out row beside the BPM, as GPUI shows it. -->
      <Fader key={tempoKey} spec={controls[tempoKey]} value={rate} stamp={snapshot?.receivedAt ?? 0} resettable={false} />
    </div>
  {/if}
  <div class="main">
    <div class="readouts">
      <span class="tag"><DeckTag {deck} /></span>
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
      <div class="tabs" role="group" aria-label="Deck drawer">
        <button type="button" class="tab" class:on={tab === 'cues'} aria-pressed={tab === 'cues'} onclick={() => (tab = 'cues')}>
          Cues
        </button>
        <button type="button" class="tab" class:on={tab === 'stems'} aria-pressed={tab === 'stems'} onclick={() => (tab = 'stems')}>
          Stems
        </button>
      </div>
    </div>
    {#if tab === 'stems'}
      <StemsPanel {deck} {stems} tint={ink?.cues[0] ?? null} {loaded} />
    {:else}
      <HotcuePads {deck} {controls} {info} {ink} {loaded} />
    {/if}
    <LoopRow {deck} {controls} {info} {frame} />
    <Transport {deck} {controls} {frame} {mirrored} />
  </div>
</div>

<style>
  .panel {
    display: grid;
    grid-template-columns: var(--om-deck-pitch-width) minmax(0, 1fr);
    grid-template-areas: 'tempo main';
    gap: var(--om-gap);
    min-width: 0;
    min-height: 0;
    padding: var(--om-gap);
    background: var(--om-surface);
  }

  .mirrored {
    grid-template-columns: minmax(0, 1fr) var(--om-deck-pitch-width);
    grid-template-areas: 'main tempo';
  }

  /* Deck A's fader sits on the left (outer) edge, deck B's on the right. */
  .tempo {
    grid-area: tempo;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    justify-items: center;
    min-height: 0;
  }

  /* A size container: the loop row folds to two rows where eight targets do not fit. */
  .main {
    grid-area: main;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto;
    gap: var(--om-gap);
    min-width: 0;
    min-height: 0;
    container-type: inline-size;
  }

  .readouts {
    display: flex;
    align-items: flex-end;
    gap: var(--om-density-space-5);
    padding-bottom: var(--om-inner-gap);
    border-bottom: var(--om-border-width) solid var(--om-border);
  }

  /* The deck's tag sits at the panel's outer end, level with the BPM digits. */
  .tag {
    align-self: flex-end;
    margin-bottom: calc((var(--om-type-display) - var(--om-type-numeral) * 1.618) / 2);
  }

  /* Deck B reads from its outer edge: tag, BPM, pitch, elapsed, right-aligned. */
  .mirrored .readouts {
    flex-direction: row-reverse;
    text-align: right;
  }

  .tabs {
    display: flex;
    gap: var(--om-inner-gap);
    margin-left: auto;
  }

  .mirrored .tabs {
    margin-left: 0;
    margin-right: auto;
  }

  .tab {
    min-height: var(--om-min-pointer-target);
    padding: 0 var(--om-inner-gap);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
    cursor: pointer;
  }

  .tab.on {
    color: var(--om-text-primary);
    border-color: var(--om-border-strong);
    background: var(--om-surface-hover);
    box-shadow: inset 0 calc(-1 * var(--om-inset-small)) 0 currentColor;
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
</style>
