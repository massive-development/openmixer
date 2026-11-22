<script lang="ts">
  import { onMount } from 'svelte';
  import {
    automixChange,
    automixPlaylists,
    automixSettings,
    type AutomixChange,
    type AutomixPage,
    type PlaylistChoice,
  } from '../ipc';

  let page: AutomixPage | null = $state(null);
  let playlists: PlaylistChoice[] = $state([]);
  let playlistsNote: string | null = $state(null);
  let error: string | null = $state(null);
  let busy = $state(false);

  // The queue source as one choice: `library`, `search` or `playlist:<id>`.
  const queueValue = (p: AutomixPage): string => (p.queue.kind === 'playlist' ? `playlist:${p.queue.playlist}` : p.queue.kind);

  async function change(c: AutomixChange): Promise<void> {
    busy = true;
    try {
      page = await automixChange(c);
      error = null;
    } catch (e) {
      error = String(e);
      page = await automixSettings().catch(() => page);
    } finally {
      busy = false;
    }
  }

  function pickQueue(value: string): void {
    if (value.startsWith('playlist:')) void change({ field: 'queue', value: { kind: 'playlist', playlist: Number(value.slice(9)) } });
    else void change({ field: 'queue', value: { kind: value === 'search' ? 'search' : 'library' } });
  }

  onMount(() => {
    automixSettings().then(
      (p) => (page = p),
      (e: unknown) => (error = String(e)),
    );
    automixPlaylists().then(
      (list) => (playlists = list),
      () => (playlistsNote = 'Playlists are not available: no library is open.'),
    );
  });
</script>

{#if page}
  <form onsubmit={(e) => e.preventDefault()} aria-busy={busy}>
    <label class="check">
      <input type="checkbox" checked={page.enabled} disabled={busy} onchange={(e) => void change({ field: 'enabled', value: e.currentTarget.checked })} />
      <span>Automix on</span>
    </label>
    <label>
      <span>Style</span>
      <select value={page.style} disabled={busy} onchange={(e) => void change({ field: 'style', value: Number(e.currentTarget.value) })}>
        {#each page.styles as label, code (code)}<option value={code}>{label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Transition length</span>
      <select value={page.lengthBeats} disabled={busy} onchange={(e) => void change({ field: 'lengthBeats', value: Number(e.currentTarget.value) })}>
        {#each page.lengths as l (l.beats)}<option value={l.beats}>{l.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Queue</span>
      <select value={queueValue(page)} disabled={busy} onchange={(e) => pickQueue(e.currentTarget.value)}>
        <option value="library">Whole library</option>
        <option value="search">Current search</option>
        {#each playlists as p (p.id)}<option value="playlist:{p.id}">{p.label}</option>{/each}
        {#if page.queue.kind === 'playlist' && !playlists.some((p) => p.id === page?.queue.playlist)}
          <option value="playlist:{page.queue.playlist}">Playlist {page.queue.playlist}</option>
        {/if}
      </select>
    </label>
    {#if playlistsNote}<p class="note">{playlistsNote}</p>{/if}
    <label class="check">
      <input type="checkbox" checked={page.smart} disabled={busy} onchange={(e) => void change({ field: 'smart', value: e.currentTarget.checked })} />
      <span>Smart pick (prefer matching key and tempo)</span>
    </label>
    <label>
      <span>Largest tempo gap</span>
      <span class="gap">
        <input
          type="range"
          min="0"
          max={page.maxBpmGapPercent}
          value={page.minBpmGapPercent}
          disabled={busy}
          aria-label="Largest tempo gap in percent"
          onchange={(e) => void change({ field: 'minBpmGapPercent', value: Number(e.currentTarget.value) })}
        />
        <span class="num">{page.minBpmGapPercent}%</span>
      </span>
    </label>
    <label class="check">
      <input type="checkbox" checked={page.autostartAtEnd} disabled={busy} onchange={(e) => void change({ field: 'autostartAtEnd', value: e.currentTarget.checked })} />
      <span>Start the next track by itself when the playing one nears its end</span>
    </label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else if error}
  <p class="error" role="alert">{error}</p>
{:else}
  <p class="note">Loading Automix settings…</p>
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

  label.check {
    display: flex;
    align-items: center;
    gap: var(--om-inner-gap);
    color: var(--om-text-primary);
  }

  select {
    appearance: none;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font: inherit;
  }

  select:disabled,
  input:disabled {
    opacity: 0.5;
  }

  select:focus-visible,
  input:focus-visible {
    outline: none;
    box-shadow: var(--om-focus-ring);
  }

  .gap {
    display: flex;
    align-items: center;
    gap: var(--om-inner-gap);
  }

  .gap input {
    flex: 1;
  }

  .num {
    min-width: 4ch;
    font-family: var(--om-font-mono);
    font-size: var(--om-type-numeral);
    color: var(--om-text-primary);
  }

  .note {
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
