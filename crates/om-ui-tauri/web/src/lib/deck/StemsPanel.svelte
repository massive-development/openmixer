<script lang="ts">
  import { stemAllOn, stemFilter, stemMute, stemPrepare, stemSolo, stemToggle, stemVolume, type StemsState } from '../ipc';
  import { filterText } from '../sampler/pads';

  interface Props {
    deck: number;
    stems: StemsState | null;
    /** The deck's tint, for the activity fill of a playing stem. */
    tint: string | null;
    /** An empty deck has nothing to separate. */
    loaded: boolean;
  }

  let { deck, stems, tint, loaded }: Props = $props();

  const live = $derived(stems?.live ?? false);
  const message = $derived.by(() => {
    if (!stems) return 'Stems are not available.';
    if (stems.note) return stems.note;
    if (!loaded) return 'Load a track first.';
    if (stems.preparing !== null) return `Preparing stems ${stems.preparing}%`;
    if (stems.cached) return 'Stems are ready. Load them to play from stems.';
    if (stems.label === 'Failed') return 'Preparing the stems failed.';
    return stems.canPrepare ? 'No stems for this track yet. Prepare them to split it.' : 'No stems for this track.';
  });
</script>

<div class="stems" role="group" aria-label="Stems">
  <div class="head">
    <p class="status" role="status">{live ? 'Playing from stems' : message}</p>
    <div class="actions">
      <button
        type="button"
        class="btn"
        disabled={!stems || !loaded || (!live && !stems.cached)}
        onclick={() => stemToggle(deck)}
        title="Stems can change only while the deck is stopped"
      >
        {live ? 'Stems off' : 'Load stems'}
      </button>
      <button
        type="button"
        class="btn"
        disabled={!stems || !loaded || !stems.canPrepare || stems.cached || stems.preparing !== null}
        onclick={() => stemPrepare(deck)}
      >
        Prepare
      </button>
      <button type="button" class="btn" disabled={!live} onclick={() => stemAllOn(deck)} title="Unmute all, unity volume, filters off">
        All on
      </button>
    </div>
  </div>
  <div class="rows">
    {#each stems?.stems ?? [] as stem, i (stem.name)}
      <div class="row" class:idle={!live}>
        <button
          type="button"
          class="btn name"
          class:lit={live && !stem.muted}
          aria-pressed={live && !stem.muted}
          aria-label="{stem.name}, {stem.muted ? 'muted' : 'playing'}"
          disabled={!live}
          style:--fill={tint}
          onclick={() => stemMute(deck, i)}
        >
          <span class="fill" style:width="{Math.round(stem.level * 100)}%"></span>
          <span class="word">{stem.name}</span>
        </button>
        <button
          type="button"
          class="btn solo"
          class:lit={stem.soloed}
          aria-pressed={stem.soloed}
          aria-label="Solo {stem.name}"
          disabled={!live}
          onclick={() => stemSolo(deck, i)}
        >
          S
        </button>
        <label class="slider">
          <span class="label">Vol</span>
          <input
            type="range"
            min="0"
            max={stems?.maxVolume ?? 1.5}
            step="0.01"
            value={stem.volume}
            disabled={!live}
            aria-label="{stem.name} volume"
            aria-valuetext="{Math.round(stem.volume * 100)}%"
            oninput={(e) => stemVolume(deck, i, e.currentTarget.valueAsNumber)}
            ondblclick={() => stemVolume(deck, i, 1)}
          />
        </label>
        <label class="slider">
          <span class="label">Filter</span>
          <input
            type="range"
            min="-1"
            max="1"
            step="0.01"
            value={stem.filter}
            disabled={!live}
            aria-label="{stem.name} filter"
            aria-valuetext={filterText(stem.filter)}
            oninput={(e) => stemFilter(deck, i, e.currentTarget.valueAsNumber)}
            ondblclick={() => stemFilter(deck, i, 0)}
          />
        </label>
      </div>
    {/each}
  </div>
</div>

<style>
  .stems {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: var(--om-density-space-1);
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--om-inner-gap);
  }

  .status {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    color: var(--om-text-secondary);
    font-size: var(--om-type-label);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .actions {
    display: flex;
    gap: var(--om-inner-gap);
  }

  .rows {
    display: grid;
    grid-template-rows: repeat(4, minmax(var(--om-min-pointer-target), 1fr));
    gap: var(--om-density-space-1);
    min-height: 0;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(5em, 1fr) var(--om-min-pointer-target) minmax(0, 2fr) minmax(0, 2fr);
    align-items: stretch;
    gap: var(--om-inner-gap);
  }

  .btn {
    position: relative;
    min-width: var(--om-min-pointer-target);
    min-height: var(--om-min-pointer-target);
    padding: 0 var(--om-inner-gap);
    overflow: hidden;
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
    white-space: nowrap;
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    background: var(--om-surface-hover);
    color: var(--om-text-primary);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .lit {
    color: var(--om-text-primary);
    border-color: var(--om-border-strong);
    box-shadow: inset 0 calc(-1 * var(--om-inset-small)) 0 currentColor;
  }

  /* The activity fill sits under the word, at the deck's tint. */
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--fill, var(--om-accent-0));
    opacity: var(--om-deck-tint-alpha);
    pointer-events: none;
  }

  .word {
    position: relative;
  }

  .slider {
    display: grid;
    grid-template-columns: 3em minmax(0, 1fr);
    align-items: center;
    gap: var(--om-inner-gap);
  }

  .label {
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    text-transform: uppercase;
  }

  input[type='range'] {
    min-width: 0;
    accent-color: var(--om-accent-0);
  }

  input:disabled {
    opacity: 0.5;
  }
</style>
