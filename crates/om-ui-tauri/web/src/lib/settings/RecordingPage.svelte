<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { recordChange, recordSettings, type RecordChange, type RecordPage } from '../ipc';

  let page: RecordPage | null = $state(null);
  let error: string | null = $state(null);
  let busy = $state(false);
  // The folder as typed; it is applied on Enter or when the field loses focus.
  let typed = $state('');

  async function change(c: RecordChange): Promise<void> {
    busy = true;
    try {
      page = await recordChange(c);
      typed = page.folder;
      error = null;
    } catch (e) {
      error = String(e);
      page = await recordSettings().catch(() => page);
      typed = page?.folder ?? '';
    } finally {
      busy = false;
    }
  }

  function applyFolder(): void {
    if (page && typed.trim() !== page.folder) void change({ field: 'folder', value: typed });
  }

  async function browse(): Promise<void> {
    try {
      const chosen = await open({ directory: true, multiple: false, defaultPath: page?.folder || page?.defaultFolder });
      if (typeof chosen === 'string') await change({ field: 'folder', value: chosen });
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    recordSettings().then(
      (p) => {
        page = p;
        typed = p.folder;
      },
      (e: unknown) => (error = String(e)),
    );
  });
</script>

{#if page}
  <form onsubmit={(e) => (e.preventDefault(), applyFolder())} aria-busy={busy}>
    <label>
      <span>Folder</span>
      <span class="folder">
        <input
          type="text"
          bind:value={typed}
          placeholder={page.defaultFolder}
          disabled={busy}
          spellcheck="false"
          onblur={applyFolder}
        />
        <button type="button" disabled={busy} onclick={() => void browse()}>Browse</button>
      </span>
    </label>
    <p class="note">Leave it empty to record to {page.defaultFolder}.</p>
    <label>
      <span>Format</span>
      <select value={page.format} disabled={busy} onchange={(e) => void change({ field: 'format', value: e.currentTarget.value })}>
        {#each page.formats as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" checked={page.stopOnLowDisk} disabled={busy} onchange={(e) => void change({ field: 'stopOnLowDisk', value: e.currentTarget.checked })} />
      <span>Stop a recording that runs the disk below 200 MB</span>
    </label>
    <p class="note">A change applies to the next recording.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else if error}
  <p class="error" role="alert">{error}</p>
{:else}
  <p class="note">Loading recording settings…</p>
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

  .folder {
    display: flex;
    gap: var(--om-inner-gap);
  }

  input[type='text'],
  select,
  button {
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-primary);
    font: inherit;
  }

  input[type='text'] {
    flex: 1;
    min-width: 0;
  }

  select {
    appearance: none;
  }

  button {
    cursor: pointer;
  }

  select:disabled,
  input:disabled,
  button:disabled {
    opacity: 0.5;
  }

  select:focus-visible,
  input:focus-visible,
  button:focus-visible {
    outline: none;
    box-shadow: var(--om-focus-ring);
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
