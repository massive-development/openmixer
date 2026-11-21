<script lang="ts">
  import { onMount } from 'svelte';
  import { audioChange, audioSettings, type AudioChange, type AudioPage } from '../ipc';

  let page: AudioPage | null = $state(null);
  // Why the last edit was refused, shown as the shell wrote it.
  let error: string | null = $state(null);
  let busy = $state(false);

  async function run(next: Promise<AudioPage>): Promise<void> {
    busy = true;
    try {
      page = await next;
      error = null;
    } catch (e) {
      error = String(e);
      // The device may now differ from what the choices show; read it again.
      page = await audioSettings().catch(() => page);
    } finally {
      busy = false;
    }
  }

  const change = (c: AudioChange): Promise<void> => run(audioChange(c));
  // A pair select's value: the pair's index, or "none" for no cue output.
  const cueValue = (v: string): number | null => (v === 'none' ? null : Number(v));

  onMount(() => void run(audioSettings()));
</script>

{#if page}
  <form onsubmit={(e) => e.preventDefault()} aria-busy={busy}>
    {#if page.mock}
      <p class="note" role="status">The silent mock device is running; choices here do not reach a sound card.</p>
    {/if}
    {#if page.deviceTypes.length > 0}
      <label>
        <span>Audio system</span>
        <select value={page.deviceType} disabled={busy} onchange={(e) => void change({ field: 'deviceType', value: e.currentTarget.value })}>
          {#each page.deviceTypes as t (t)}<option value={t}>{t}</option>{/each}
        </select>
      </label>
    {/if}
    <label>
      <span>Output device</span>
      <select value={page.device} disabled={busy} onchange={(e) => void change({ field: 'device', value: e.currentTarget.value })}>
        {#each page.devices as d (d)}<option value={d}>{d}</option>{/each}
      </select>
    </label>
    <label>
      <span>Sample rate</span>
      <select value={page.sampleRate} disabled={busy} onchange={(e) => void change({ field: 'sampleRate', value: Number(e.currentTarget.value) })}>
        {#each page.sampleRates as r (r)}<option value={r}>{r} Hz</option>{/each}
      </select>
    </label>
    <label>
      <span>Buffer size</span>
      <select value={page.bufferSize} disabled={busy} onchange={(e) => void change({ field: 'bufferSize', value: Number(e.currentTarget.value) })}>
        {#each page.bufferSizes as b (b)}<option value={b}>{b} samples</option>{/each}
      </select>
    </label>
    <label>
      <span>Master output</span>
      <select value={page.masterPair} disabled={busy || page.pairs.length === 0} onchange={(e) => void change({ field: 'masterPair', value: Number(e.currentTarget.value) })}>
        {#each page.pairs as p, i (i)}<option value={i}>{p}</option>{/each}
      </select>
    </label>
    <label>
      <span>Headphone cue output</span>
      <select value={page.cuePair ?? 'none'} disabled={busy || page.pairs.length === 0} onchange={(e) => void change({ field: 'cuePair', value: cueValue(e.currentTarget.value) })}>
        <option value="none">None</option>
        {#each page.pairs as p, i (i)}<option value={i}>{p}</option>{/each}
      </select>
    </label>
    <p class="stats num" role="status">{page.stats}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else}
  <p class="note">Loading audio settings…</p>
{/if}

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--om-gap);
  }

  label {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    align-items: center;
    gap: var(--om-gap);
    font-size: var(--om-type-label);
    color: var(--om-text-secondary);
  }

  /* Native select chrome ignores the theme in WebKitGTK; draw the box and a chevron from tokens. */
  select {
    appearance: none;
    background-image: linear-gradient(45deg, transparent 50%, var(--om-text-secondary) 50%),
      linear-gradient(135deg, var(--om-text-secondary) 50%, transparent 50%);
    background-position:
      calc(100% - 14px) 50%,
      calc(100% - 9px) 50%;
    background-size:
      5px 5px,
      5px 5px;
    background-repeat: no-repeat;
    padding-right: 28px;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font: inherit;
  }

  select:disabled {
    opacity: 0.5;
  }

  select:focus-visible {
    outline: none;
    box-shadow: var(--om-focus-ring);
  }

  .note,
  .stats {
    margin: 0;
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
  }

  .error {
    margin: 0;
    color: var(--om-warning);
    font-size: var(--om-type-label);
  }
</style>
