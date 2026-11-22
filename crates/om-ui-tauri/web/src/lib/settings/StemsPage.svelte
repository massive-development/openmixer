<script lang="ts">
  import { onMount } from 'svelte';
  import {
    stemsAcceptDownload,
    stemsCancelDownload,
    stemsClearCache,
    stemsSettings,
    stemsSettingsChange,
    type StemsPage as StemsPageData,
    type StemsPrefsChange,
  } from '../ipc';

  // While the model downloads the page follows it.
  const POLL_MS = 1000;

  let page = $state<StemsPageData | null>(null);
  let error: string | null = $state(null);
  let busy = $state(false);
  // Clearing asks again first, inline: a dialog would stop the shell's own events.
  let confirming = $state(false);
  let cleared: string | null = $state(null);
  let cap = $state('');

  const host = $derived(page?.host ?? null);
  const downloading = $derived(host?.state === 'downloading');
  const canAccept = $derived(host !== null && !host.ready && host.licence !== null && !downloading);

  function show(next: StemsPageData): void {
    page = next;
    cap = String(next.prefs.maxCacheGb);
  }

  async function run(work: Promise<StemsPageData | void>): Promise<void> {
    busy = true;
    try {
      const result = await work;
      if (result) show(result);
      else show(await stemsSettings());
      error = null;
    } catch (e) {
      error = String(e);
      page = await stemsSettings().catch(() => page);
    } finally {
      busy = false;
    }
  }

  const change = (c: StemsPrefsChange): Promise<void> => run(stemsSettingsChange(c));

  function commitCap(): void {
    const gb = Math.round(Number(cap));
    if (!Number.isFinite(gb) || !page || gb === page.prefs.maxCacheGb) {
      cap = String(page?.prefs.maxCacheGb ?? '');
      return;
    }
    void change({ field: 'maxCacheGb', value: gb });
  }

  async function clear(): Promise<void> {
    confirming = false;
    await run(
      stemsClearCache().then((n) => {
        cleared = n === 1 ? 'Removed 1 stem set.' : `Removed ${n} stem sets.`;
      }),
    );
  }

  onMount(() => {
    void run(stemsSettings());
    // Follows a download or a running separation; a quiet page asks nothing.
    const timer = setInterval(() => {
      if (busy || !(downloading || (host?.inFlight ?? 0) > 0)) return;
      stemsSettings().then(show, () => {});
    }, POLL_MS);
    return () => clearInterval(timer);
  });
</script>

{#if page && host}
  <form onsubmit={(e) => e.preventDefault()} aria-busy={busy}>
    <p class="note">
      Stems are prepared for the tracks you pick (Split stems in the library, or Prepare on a deck) or, with
      auto-prepare on, for tracks analysed after they were added. Loading a track never starts a separation.
    </p>

    <h3>Stem model</h3>
    <p class="status" role="status">{host.line}</p>
    {#if downloading && host.progress !== null}
      <progress value={host.progress} max="1" aria-label="Model download"></progress>
    {/if}
    {#if host.licence && !host.ready}
      <section class="licence" aria-label="Licence of {host.licence.name}">
        <h4>{host.licence.name}</h4>
        <pre>{host.licence.text}</pre>
      </section>
    {/if}
    <div class="row">
      {#if downloading}
        <button type="button" onclick={() => void run(stemsCancelDownload())} disabled={busy}>Cancel download</button>
      {:else if canAccept}
        <button type="button" onclick={() => void run(stemsAcceptDownload())} disabled={busy}>Accept licence and download</button>
      {/if}
    </div>

    <h3>Cache</h3>
    <p class="status num">{page.usage}</p>
    <label>
      <span>Cache size (GB)</span>
      <input
        type="number"
        min="1"
        max={page.prefs.maxCacheLimit}
        step="1"
        bind:value={cap}
        disabled={busy}
        onchange={commitCap}
        onkeydown={(e) => e.key === 'Enter' && commitCap()}
      />
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={page.prefs.autoPrepare}
        disabled={busy || !host.available}
        onchange={(e) => void change({ field: 'autoPrepare', value: e.currentTarget.checked })}
      />
      <span>Prepare stems for newly analysed tracks</span>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={page.prefs.autoLoad}
        disabled={busy}
        onchange={(e) => void change({ field: 'autoLoad', value: e.currentTarget.checked })}
      />
      <span>Load a track's stems with it (from the next start)</span>
    </label>
    <div class="row">
      {#if confirming}
        <span class="status">Remove every prepared stem set?</span>
        <button type="button" onclick={() => void clear()} disabled={busy}>Remove</button>
        <button type="button" onclick={() => (confirming = false)}>Keep</button>
      {:else}
        <button
          type="button"
          onclick={() => {
            confirming = true;
            cleared = null;
          }}
          disabled={busy || !host.available}
        >
          Clear stem cache…
        </button>
        {#if cleared}<span class="status" role="status">{cleared}</span>{/if}
      {/if}
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else}
  <p class="note">Loading stems settings…</p>
{/if}

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--om-gap);
  }

  h3,
  h4 {
    margin: 0;
    color: var(--om-text-primary);
    font-size: var(--om-type-body);
  }

  .note,
  .status {
    margin: 0;
    color: var(--om-text-secondary);
    font-size: var(--om-type-label);
  }

  .licence pre {
    margin: var(--om-inner-gap) 0 0;
    max-height: 160px;
    overflow: auto;
    padding: var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-well);
    color: var(--om-text-secondary);
    font-family: var(--om-font-mono);
    font-size: var(--om-type-label);
    white-space: pre-wrap;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--om-inner-gap);
  }

  label {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    align-items: center;
    gap: var(--om-gap);
    font-size: var(--om-type-label);
    color: var(--om-text-secondary);
  }

  label.check {
    grid-template-columns: auto minmax(0, 1fr);
  }

  input[type='number'] {
    width: 8em;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font: inherit;
  }

  input:focus-visible,
  button:focus-visible {
    outline: none;
    box-shadow: var(--om-focus-ring);
  }

  button {
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font: inherit;
    font-size: var(--om-type-label);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: var(--om-surface-hover);
    color: var(--om-text-primary);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  progress {
    width: 100%;
    accent-color: var(--om-accent-0);
  }

  .error {
    margin: 0;
    color: var(--om-warning);
    font-size: var(--om-type-label);
  }
</style>
