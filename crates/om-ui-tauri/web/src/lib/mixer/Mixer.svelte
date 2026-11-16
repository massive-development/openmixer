<script lang="ts">
  import { onMount } from 'svelte';
  import Fader from '../controls/Fader.svelte';
  import Knob from '../controls/Knob.svelte';
  import Meter from '../controls/Meter.svelte';
  import { themeNumber } from '../dom';
  import type { Controls } from '../ipc';
  import type { MeterScale } from '../meter';
  import type { Snapshot } from '../packet';
  import ChannelStrip from './ChannelStrip.svelte';

  interface Props {
    controls: Controls;
    snapshot: Snapshot | null;
  }

  let { controls, snapshot }: Props = $props();

  // The bars' dB scale is the theme's, read once the theme's variables are in place.
  let scale: MeterScale = $state({ floorDb: Number.NaN, midDb: Number.NaN, highDb: Number.NaN });

  const stamp = $derived(snapshot?.receivedAt ?? 0);
  const master = $derived(controls['mixer.master']);
  const crossfader = $derived(controls['mixer.crossfader']);

  onMount(() => {
    scale = {
      floorDb: themeNumber('--om-meter-floor-db'),
      midDb: themeNumber('--om-meter-mid-db'),
      highDb: themeNumber('--om-meter-high-db'),
    };
  });
</script>

<div class="mixer">
  <div class="strips">
    <ChannelStrip channel={0} {controls} frame={snapshot?.channels[0]} {stamp} {scale} />
    <div class="master">
      <h3>{master?.label ?? ''}</h3>
      <div class="meter">
        <Meter left={snapshot?.master.left ?? 0} right={snapshot?.master.right ?? 0} {scale} />
      </div>
      {#if master}
        <Knob key="mixer.master" spec={master} value={snapshot?.master.gain ?? 0} {stamp} />
      {/if}
    </div>
    <ChannelStrip channel={1} {controls} frame={snapshot?.channels[1]} {stamp} {scale} mirrored />
  </div>
  {#if crossfader}
    <div class="xf">
      <span class="end" style:color="var(--om-accent-0)">A</span>
      <Fader key="mixer.crossfader" spec={crossfader} value={snapshot?.master.crossfader ?? 0} {stamp} horizontal />
      <span class="end" style:color="var(--om-accent-1)">B</span>
    </div>
  {/if}
</div>

<style>
  .mixer {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: var(--om-inner-gap);
    min-width: 0;
    min-height: 0;
    padding: var(--om-gap);
    background: var(--om-surface);
  }

  .strips {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: var(--om-gap);
    min-height: 0;
  }

  .master {
    display: grid;
    grid-template-rows: var(--om-mixer-title-height) minmax(0, 1fr) auto;
    justify-items: center;
    gap: var(--om-inner-gap);
    min-height: 0;
  }

  h3 {
    margin: 0;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    line-height: var(--om-mixer-title-height);
    text-transform: uppercase;
  }

  .meter {
    min-height: 0;
  }

  .xf {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--om-gap);
  }

  .end {
    font-family: var(--om-font-mono);
    font-size: var(--om-type-body);
    font-weight: 700;
  }
</style>
