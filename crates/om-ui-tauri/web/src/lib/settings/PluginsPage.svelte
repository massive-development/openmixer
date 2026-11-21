<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { openPluginWindow, paramFraction, paramValue, pluginsChange, pluginsPage, failure, type PluginParam, type PluginsChange } from './devices';
  import { Pager } from './pager.svelte';
  import './shared.css';

  // Latency and values a plugin's own window changed follow within this many milliseconds.
  const POLL_MS = 1000;
  const SLIDER_STEPS = 1000;

  const pager = new Pager(pluginsPage);
  const page = $derived(pager.page);
  // The plugin whose generic parameter panel is open.
  let openParams: string | null = $state(null);
  // A slider being dragged shows its own value until it is let go.
  let dragging: Record<string, number> = $state({});

  const change = (c: PluginsChange): Promise<void> => pager.send(() => pluginsChange(c));
  const key = (id: string, param: string): string => `${id}\u0000${param}`;
  const shown = (id: string, p: PluginParam): number => dragging[key(id, p.id)] ?? p.value;
  const shownText = (id: string, p: PluginParam): string => {
    const value = dragging[key(id, p.id)];
    return value === undefined ? p.text : String(Number(value.toFixed(3)));
  };

  function release(id: string, p: PluginParam, value: number): void {
    const { [key(id, p.id)]: _, ...rest } = dragging;
    dragging = rest;
    void change({ field: 'setParam', value: { id, param: p.id, value } });
  }

  async function openWindow(id: string): Promise<void> {
    try {
      pager.notice = { text: await openPluginWindow(id), error: false };
    } catch (error) {
      pager.notice = failure(error);
    }
  }

  onMount(() => pager.start(POLL_MS));
  onDestroy(() => pager.stop());
</script>

{#if !page}
  <p class="s-note">Loading plugins…</p>
{:else if !page.available}
  <p class="s-note" role="status">Plugins are not available.</p>
{:else}
  <div class="s-stack" aria-busy={pager.busy}>
    <section class="s-stack" aria-label="Loaded plugins">
      <h3 class="s-title">Loaded plugins</h3>
      {#each page.loaded as plugin (plugin.id)}
        <div class="s-row plugin">
          <span class="name">{plugin.id}</span>
          {#if plugin.latency}<span class="s-note num">{plugin.latency}</span>{/if}
          {#if plugin.hasWindow}
            <button type="button" class="s-btn" onclick={() => void openWindow(plugin.id)}>Window</button>
          {/if}
          <button
            type="button"
            class="s-btn"
            class:on={openParams === plugin.id}
            aria-expanded={openParams === plugin.id}
            onclick={() => (openParams = openParams === plugin.id ? null : plugin.id)}
          >Parameters</button>
          <button type="button" class="s-btn" onclick={() => void change({ field: 'unload', value: plugin.id })}>Unload</button>
        </div>
        {#if openParams === plugin.id}
          <div class="params" role="group" aria-label="{plugin.id} parameters">
            {#each plugin.params as p (p.id)}
              <label class="param">
                <span class="name">{p.name}</span>
                <input
                  type="range"
                  min="0"
                  max={SLIDER_STEPS}
                  value={Math.round(paramFraction({ ...p, value: shown(plugin.id, p) }) * SLIDER_STEPS)}
                  aria-valuetext={shownText(plugin.id, p)}
                  oninput={(e) => (dragging[key(plugin.id, p.id)] = paramValue(p, Number(e.currentTarget.value) / SLIDER_STEPS))}
                  onchange={(e) => release(plugin.id, p, paramValue(p, Number(e.currentTarget.value) / SLIDER_STEPS))}
                />
                <span class="value num">{shownText(plugin.id, p)}</span>
                <button type="button" class="s-btn" onclick={() => release(plugin.id, p, p.default)}>Reset</button>
              </label>
            {:else}
              <p class="s-note">This plugin has no parameters.</p>
            {/each}
          </div>
        {/if}
      {:else}
        <p class="s-note">No plugin is loaded.</p>
      {/each}
    </section>

    <section class="s-stack" aria-label="Built-in plugins">
      <h3 class="s-title">Built-in plugins</h3>
      {#each page.builtins as b (b.id)}
        <div class="s-row plugin">
          <span class="name">{b.label} ({b.id})</span>
          <button type="button" class="s-btn" onclick={() => void change({ field: 'loadBuiltin', value: b.id })}>Load</button>
        </div>
      {:else}
        <p class="s-note">No built-in plugin to load.</p>
      {/each}
    </section>

    <section class="s-stack" aria-label="Plugin folder">
      <h3 class="s-title">Folder</h3>
      <div class="s-row">
        <button type="button" class="s-btn" onclick={() => void change({ field: 'reload' })}>Reload the plugin folder</button>
      </div>
      <p class="s-note path">
        Plugin folder: {page.folder}. Shared-library (C ABI) plugins in it are loaded; any that fail are listed with the reason.
        {page.themeCount} plugin theme(s) are in Appearance.
      </p>
    </section>

    {#if page.clap}
      {@const clap = page.clap}
      <section class="s-stack" aria-label="CLAP plugins">
        <h3 class="s-title">CLAP plugins</h3>
        {#if clap.scan === 'unasked'}
          <p class="s-note">May OpenMixer scan the standard CLAP folders? Until you answer, only the plugin folder is scanned.</p>
        {/if}
        <div class="s-row">
          <button type="button" class="s-btn" class:on={clap.scan === 'allowed'} aria-pressed={clap.scan === 'allowed'} onclick={() => void change({ field: 'clapScan', value: clap.scan !== 'allowed' })}>
            Standard folders: {clap.scan === 'allowed' ? 'scanned' : 'not scanned'}
          </button>
          {#if clap.waiting > 0 && !clap.warningAccepted}
            <button type="button" class="s-btn" onclick={() => void change({ field: 'clapAcceptWarning' })}>Accept the warning and load</button>
          {/if}
        </div>
        <p class="s-note">
          {clap.warningAccepted
            ? 'The in-process warning was accepted.'
            : 'A CLAP plugin runs its native code inside OpenMixer. CLAP plugins are not loaded until you accept the warning.'}
        </p>
        <div class="s-list files">
          {#each clap.files as f (f)}<div class="file">{f}</div>{:else}<div class="s-note file">No .clap file found.</div>{/each}
        </div>
        {#if clap.standardFolders.length > 0}
          <p class="s-note path">Standard folders: {clap.standardFolders.join(', ')}</p>
        {/if}
      </section>
    {/if}

    {#if pager.notice}<p class="s-notice" class:error={pager.notice.error} role="status">{pager.notice.text}</p>{/if}
  </div>
{/if}

<style>
  h3 {
    font-weight: normal;
  }

  .plugin .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--om-type-body);
    color: var(--om-text-primary);
  }

  .params {
    display: flex;
    flex-direction: column;
    gap: var(--om-density-space-1);
    padding: var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
  }

  .param {
    display: grid;
    grid-template-columns: minmax(80px, 1fr) minmax(120px, 2fr) 64px auto;
    align-items: center;
    gap: var(--om-inner-gap);
    font-size: var(--om-type-label);
    color: var(--om-text-secondary);
  }

  .value {
    text-align: center;
    color: var(--om-text-primary);
  }

  .path {
    overflow-wrap: anywhere;
  }

  .files {
    max-height: 90px;
    padding: var(--om-density-space-1) var(--om-inner-gap);
  }

  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--om-type-label);
    color: var(--om-text-secondary);
  }
</style>
