<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { onDestroy, onMount } from 'svelte';
  import { midiChange, midiPage, type MidiChange } from './devices';
  import { Pager } from './pager.svelte';
  import './shared.css';

  // A controller plugged in with the page open shows up within this many milliseconds.
  const POLL_MS = 1000;

  const pager = new Pager(midiPage);
  const page = $derived(pager.page);
  let target = $state('');

  const change = (c: MidiChange): Promise<void> => pager.send(() => midiChange(c));
  const learning = $derived(page?.learning ?? null);
  const labelOf = (id: string): string => page?.targets.find((t) => t.id === id)?.label ?? id;

  async function chooseMap(): Promise<void> {
    const file = await open({ multiple: false, directory: false, filters: [{ name: 'Controller map', extensions: ['json'] }] });
    if (typeof file === 'string') await change({ field: 'loadMap', value: file });
  }

  onMount(() => pager.start(POLL_MS));
  onDestroy(() => pager.stop());
  // The first target is the one a Learn press takes until another is picked.
  $effect(() => {
    if (!target && page && page.targets.length > 0) target = page.targets[0].id;
  });
</script>

{#if !page}
  <p class="s-note">Loading MIDI settings…</p>
{:else if !page.available}
  <p class="s-note" role="status">MIDI is not available (see the terminal for the reason).</p>
{:else}
  <div class="s-stack" aria-busy={pager.busy}>
    <fieldset class="inputs">
      <legend class="s-title">Input devices</legend>
      <label class="s-check">
        <input type="checkbox" checked={page.enableAll} onchange={(e) => void change({ field: 'enableAll', value: e.currentTarget.checked })} />
        All inputs
      </label>
      {#each page.inputs as name (name)}
        <label class="s-check">
          <input
            type="checkbox"
            checked={page.enableAll || page.enabledInputs.includes(name)}
            disabled={page.enableAll}
            onchange={() => void change({ field: 'toggleInput', value: name })}
          />
          {name}
        </label>
      {/each}
      {#if page.inputs.length === 0}<p class="s-note">No MIDI input device is listed.</p>{/if}
      <p class="s-note">Turn off All inputs to choose the devices one by one.</p>
    </fieldset>

    <section class="s-stack" aria-label="Controller map">
      <h3 class="s-title">Controller map</h3>
      <p class="s-note path">Map file: {page.mapFile}</p>
      <div class="s-row">
        <button type="button" class="s-btn" onclick={() => void chooseMap()}>Load map file…</button>
        <button type="button" class="s-btn" onclick={() => void change({ field: 'loadCurrentMap' })}>Load the current map</button>
        <button type="button" class="s-btn" onclick={() => void change({ field: 'useDefaultMap' })}>Use the default map</button>
      </div>
    </section>

    <section class="s-stack" aria-label="MIDI learn">
      <h3 class="s-title">MIDI learn: pick a target, then move a control</h3>
      <div class="s-row">
        <select class="s-field" aria-label="Learn target" bind:value={target} disabled={learning !== null}>
          {#each page.targets as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
        </select>
        {#if learning !== null}
          <button type="button" class="s-btn on" onclick={() => void change({ field: 'cancelLearn' })}>Cancel learn</button>
          <span class="s-note" role="status">Learning {labelOf(learning)}…</span>
        {:else}
          <button type="button" class="s-btn" disabled={!target} onclick={() => void change({ field: 'learn', value: target })}>Learn</button>
        {/if}
      </div>
    </section>

    {#if pager.notice}<p class="s-notice" class:error={pager.notice.error} role="status">{pager.notice.text}</p>{/if}
  </div>
{/if}

<style>
  .inputs {
    display: flex;
    flex-direction: column;
    gap: var(--om-density-space-1);
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }

  h3 {
    font-weight: normal;
  }

  .path {
    overflow-wrap: anywhere;
  }
</style>
