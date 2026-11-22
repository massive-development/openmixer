<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { AUDIO_EXTENSIONS } from '../loading.svelte';
  import {
    samplerClear,
    samplerCycleMode,
    samplerLoad,
    samplerPress,
    samplerRelease,
    samplerSelect,
    samplerSetGain,
    samplerSetPitch,
    samplerStopAll,
    type SamplerState,
  } from '../ipc';
  import { gainText, padForKey, pitchText, isTyping } from './pads';

  interface Props {
    sampler: SamplerState | null;
    onclose: () => void;
  }

  let { sampler, onclose }: Props = $props();

  const pads = $derived(sampler?.pads ?? []);
  const chosen = $derived(pads.find((p) => p.selected) ?? null);
  // The pad a pointer or key holds; a gate pad plays only while it is held.
  let held: number | null = null;

  function press(pad: number): void {
    if (!sampler || held === pad) return;
    if (held !== null) samplerRelease();
    held = pad;
    samplerPress(pad, sampler.pressVelocity);
  }

  function release(pad?: number): void {
    if (held === null || (pad !== undefined && pad !== held)) return;
    held = null;
    samplerRelease();
  }

  function onPointerDown(pad: number, event: PointerEvent): void {
    if (event.button !== 0) return;
    // The release must come even when the pointer ends outside the pad.
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    press(pad);
  }

  // Keys 1 to 8 press pads while the strip is open and no text field has the keyboard.
  function onKeyDown(event: KeyboardEvent): void {
    if (event.repeat || event.ctrlKey || event.metaKey || event.altKey || isTyping(event.target)) return;
    const pad = padForKey(pads, event.key);
    if (pad === null) return;
    event.preventDefault();
    press(pad);
  }

  function onKeyUp(event: KeyboardEvent): void {
    const pad = padForKey(pads, event.key);
    if (pad !== null) release(pad);
  }

  // Window blur ends a held key whose release the page never sees.
  function onBlur(): void {
    release();
  }

  async function load(): Promise<void> {
    if (!chosen) return;
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Audio', extensions: AUDIO_EXTENSIONS }],
    });
    if (typeof path === 'string') samplerLoad(chosen.pad, path);
  }

  const caption = $derived(sampler?.status ?? sampler?.caption ?? '');
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} onblur={onBlur} />

<div class="strip" role="group" aria-label="Sampler" data-sampler>
  {#if sampler && sampler.available}
    <div class="grid" role="group" aria-label="Pads">
      {#each pads as pad (pad.pad)}
        <button
          type="button"
          class="pad"
          class:playing={pad.playing}
          class:selected={pad.selected}
          class:empty={!pad.loaded}
          aria-label="Pad {pad.pad + 1}, {pad.loaded ? pad.name : 'empty'}, {pad.mode}"
          aria-pressed={pad.playing}
          title={pad.loaded ? `${pad.name}: ${pad.mode}` : 'Empty: select it, then Load'}
          data-control="sampler.pad{pad.pad}"
          data-pad={pad.pad}
          onpointerdown={(e) => onPointerDown(pad.pad, e)}
          onpointerup={() => release(pad.pad)}
          onpointercancel={() => release(pad.pad)}
          onfocus={() => !pad.selected && samplerSelect(pad.pad)}
          onkeydown={(e) => {
            // Space and Enter hit the focused pad; the digit keys are the window's.
            if (e.key === ' ' || e.key === 'Enter') {
              e.preventDefault();
              if (!e.repeat) press(pad.pad);
            }
          }}
          onkeyup={(e) => (e.key === ' ' || e.key === 'Enter') && release(pad.pad)}
        >
          <span class="key num">{pad.key}</span>
          <span class="name">{pad.loading ? 'Loading…' : pad.name}</span>
          <span class="mode">{pad.missing ? 'file missing' : pad.loaded ? pad.mode : ''}</span>
        </button>
      {/each}
    </div>
    <div class="side">
      <p class="caption" class:problem={sampler.status !== null} role="status">{caption}</p>
      <div class="buttons">
        <button type="button" class="btn" onclick={samplerCycleMode} disabled={!chosen} title={sampler.modeHint}>
          {chosen?.mode ?? 'one-shot'}
        </button>
        <button type="button" class="btn" onclick={samplerStopAll}>Stop</button>
        <button type="button" class="btn" onclick={() => void load()} disabled={!chosen}>Load…</button>
        <button type="button" class="btn" onclick={() => chosen && samplerClear(chosen.pad)} disabled={!chosen?.loaded}>
          Clear
        </button>
      </div>
      <label class="slider">
        <span class="label">Gain</span>
        <input
          type="range"
          min="0"
          max={sampler.maxGain}
          step="0.01"
          value={chosen?.gain ?? 1}
          disabled={!chosen?.loaded}
          aria-valuetext={gainText(chosen?.gain ?? 1)}
          oninput={(e) => samplerSetGain(e.currentTarget.valueAsNumber)}
          ondblclick={() => samplerSetGain(1)}
        />
        <span class="value num">{gainText(chosen?.gain ?? 1)}</span>
      </label>
      <label class="slider">
        <span class="label">Pitch</span>
        <input
          type="range"
          min={-sampler.maxPitch}
          max={sampler.maxPitch}
          step="0.1"
          value={chosen?.pitch ?? 0}
          disabled={!chosen?.loaded}
          aria-valuetext={pitchText(chosen?.pitch ?? 0)}
          oninput={(e) => samplerSetPitch(e.currentTarget.valueAsNumber)}
          ondblclick={() => samplerSetPitch(0)}
        />
        <span class="value num">{pitchText(chosen?.pitch ?? 0)}</span>
      </label>
    </div>
  {:else}
    <p class="caption problem" role="status">{sampler?.status ?? 'The sampler is not running.'}</p>
  {/if}
  <button type="button" class="btn close" onclick={onclose} aria-label="Close the sampler">Close</button>
</div>

<style>
  .strip {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--om-gap);
    height: 100%;
    padding: var(--om-gap);
    border-top: var(--om-border-width) solid var(--om-border);
    background: var(--om-surface);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    grid-template-rows: repeat(2, minmax(0, 1fr));
    gap: var(--om-inner-gap);
    min-height: 0;
  }

  .pad {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    min-height: var(--om-min-pointer-target);
    padding: var(--om-inset-large);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
    text-align: left;
    cursor: pointer;
    touch-action: none;
    user-select: none;
  }

  .pad:hover {
    background: var(--om-surface-hover);
  }

  .pad.empty {
    color: var(--om-text-muted);
  }

  .pad.selected {
    border-color: var(--om-border-strong);
  }

  /* Playing: a bar along the foot as well as the fill, so the state does not rest on colour. */
  .pad.playing {
    background: var(--om-surface-hover);
    border-color: var(--om-accent-0);
    box-shadow: inset 0 calc(-1 * var(--om-inset-small)) 0 var(--om-accent-0);
  }

  .key {
    font-size: var(--om-type-numeral);
    font-weight: 700;
  }

  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .mode {
    min-height: 1em;
    color: var(--om-text-muted);
    text-transform: uppercase;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: var(--om-inner-gap);
    min-width: 0;
    padding-right: calc(var(--om-min-pointer-target) * 2);
  }

  .caption {
    margin: 0;
    overflow: hidden;
    color: var(--om-text-secondary);
    font-size: var(--om-type-body);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .caption.problem {
    color: var(--om-text-primary);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--om-inner-gap);
  }

  .btn {
    min-width: var(--om-min-pointer-target);
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

  .btn:hover:not(:disabled) {
    background: var(--om-surface-hover);
    color: var(--om-text-primary);
  }

  .btn:disabled,
  .pad:disabled,
  input:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .close {
    position: absolute;
    top: var(--om-gap);
    right: var(--om-gap);
  }

  .slider {
    display: grid;
    grid-template-columns: 3.5em minmax(0, 1fr) 5em;
    align-items: center;
    gap: var(--om-inner-gap);
  }

  .label {
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    text-transform: uppercase;
  }

  .value {
    color: var(--om-readout);
    font-size: var(--om-type-numeral);
    text-align: right;
  }

  input[type='range'] {
    min-width: 0;
    accent-color: var(--om-accent-0);
  }
</style>
