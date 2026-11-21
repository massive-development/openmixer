<script lang="ts">
  import { recordToggle, type RecordBarState } from '../ipc';
  import type { Header } from './header.svelte';

  interface Props {
    bar: RecordBarState;
    header: Header;
  }

  let { bar, header }: Props = $props();

  const name = $derived(bar.recording ? 'Stop recording' : 'Record');
  const hint = $derived(bar.error !== '' ? bar.error : `${name} (R)`);
</script>

<!-- A control face with a red ring for a dot while idle; while recording, a filled red dot and a
     red edge (red is for recording), the elapsed time, the size, a dropped-frames badge when any,
     and the mini L/R peak bars. -->
<button
  type="button"
  class="record"
  class:recording={bar.recording}
  class:failed={bar.failed}
  aria-label={name}
  aria-pressed={bar.recording}
  title={hint}
  data-control="record.toggle"
  onclick={() => void header.press(recordToggle)}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="label num" class:time={bar.recording}>{bar.label}</span>
  {#if bar.recording}
    <span class="size num">{bar.size}</span>
  {/if}
  {#if bar.warning !== ''}
    <span class="drops num" title={bar.warning}>!{bar.droppedFrames}</span>
  {/if}
  {#if bar.recording}
    <span class="peaks" aria-hidden="true">
      <span class="track"><span class="level" style:width="{bar.peakLeft * 100}%"></span></span>
      <span class="track"><span class="level" style:width="{bar.peakRight * 100}%"></span></span>
    </span>
  {/if}
</button>
{#if bar.recording || bar.failed}
  <!-- Said once when it starts or stops, not on every tick of the clock. -->
  <span class="sr" role="status">{bar.failed ? 'Recording stopped' : 'Recording'}</span>
{/if}

<style>
  .record {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--om-density-space-1);
    min-height: var(--om-min-pointer-target);
    padding: 0 var(--om-density-space-2);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
    font-weight: 500;
    cursor: pointer;
  }

  .record:hover {
    background: var(--om-surface-hover);
  }

  .record:active {
    background: var(--om-well);
  }

  .recording {
    border-color: var(--om-playhead);
  }

  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border: var(--om-border-width) solid var(--om-playhead);
    border-radius: 50%;
  }

  .recording .dot {
    background: var(--om-playhead);
  }

  .failed .label {
    color: var(--om-meter-high);
  }

  .num {
    font-family: var(--om-font-mono);
    font-size: var(--om-type-numeral);
  }

  .label:not(.time) {
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
  }

  .size {
    color: var(--om-text-secondary);
  }

  .drops {
    padding: 0 var(--om-density-space-0);
    border-radius: var(--om-control-radius);
    background: var(--om-meter-mid);
    color: var(--om-on-accent);
  }

  .peaks {
    display: flex;
    flex-direction: column;
    gap: var(--om-density-space-0);
  }

  .track {
    width: 24px;
    height: 3px;
    border-radius: 1.5px;
    background: var(--om-well);
    overflow: hidden;
  }

  .level {
    display: block;
    height: 100%;
    background: var(--om-meter-low);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
