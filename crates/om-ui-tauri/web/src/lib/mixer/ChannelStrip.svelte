<script lang="ts">
  import Fader from '../controls/Fader.svelte';
  import Knob from '../controls/Knob.svelte';
  import Meter from '../controls/Meter.svelte';
  import type { Controls } from '../ipc';
  import type { MeterScale } from '../meter';
  import type { ChannelFrame } from '../packet';

  interface Props {
    channel: number;
    controls: Controls;
    frame: ChannelFrame | undefined;
    stamp: number;
    scale: MeterScale;
    /** Channel B mirrors channel A: knobs at the outer edge, meter by the master. */
    mirrored?: boolean;
  }

  let { channel, controls, frame, stamp, scale, mirrored = false }: Props = $props();

  /** The knob column top to bottom, as a hardware channel (`om_ui::widgets::mixer::KNOB_ORDER`). */
  const knobs = $derived([
    { key: `ch${channel}.trim`, value: frame?.trim ?? 0 },
    { key: `ch${channel}.eq.high`, value: frame?.eq[2] ?? 0 },
    { key: `ch${channel}.eq.mid`, value: frame?.eq[1] ?? 0 },
    { key: `ch${channel}.eq.low`, value: frame?.eq[0] ?? 0 },
    { key: `ch${channel}.filter`, value: frame?.filter ?? 0 },
  ]);
  const faderKey = $derived(`ch${channel}.fader`);
  const letter = $derived(String.fromCharCode(65 + channel));
</script>

<div class="strip" class:mirrored style:--hue="var(--om-accent-{channel})">
  <h3><i></i>Channel {letter}</h3>
  <div class="knobs">
    {#each knobs as knob (knob.key)}
      {#if controls[knob.key]}
        <Knob
          key={knob.key}
          spec={controls[knob.key]}
          value={knob.value}
          {stamp}
          label={controls[knob.key].label}
          {mirrored}
        />
      {/if}
    {/each}
  </div>
  <div class="fader">
    {#if controls[faderKey]}
      <Fader key={faderKey} spec={controls[faderKey]} value={frame?.fader ?? 0} {stamp} />
    {/if}
  </div>
  <div class="meter">
    <Meter left={frame?.left ?? 0} right={frame?.right ?? 0} {scale} />
  </div>
</div>

<style>
  .strip {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    grid-template-rows: var(--om-mixer-title-height) minmax(0, 1fr);
    grid-template-areas:
      'title title title'
      'knobs fader meter';
    column-gap: var(--om-inner-gap);
    min-width: 0;
    min-height: 0;
  }

  .mirrored {
    grid-template-columns: auto auto minmax(0, 1fr);
    grid-template-areas:
      'title title title'
      'meter fader knobs';
  }

  h3 {
    grid-area: title;
    display: flex;
    align-items: center;
    gap: var(--om-density-space-2);
    margin: 0;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
  }

  .mirrored h3 {
    flex-direction: row-reverse;
  }

  h3 i {
    width: var(--om-mixer-accent-dot);
    height: var(--om-mixer-accent-dot);
    border-radius: 50%;
    background: var(--hue);
  }

  .knobs {
    grid-area: knobs;
    display: grid;
    grid-template-rows: repeat(5, minmax(0, 1fr));
    align-items: center;
    min-height: 0;
  }

  .mirrored .knobs {
    justify-items: end;
  }

  .fader {
    grid-area: fader;
    min-height: 0;
  }

  .meter {
    grid-area: meter;
    min-height: 0;
    padding: var(--om-inset-large) 0;
  }
</style>
