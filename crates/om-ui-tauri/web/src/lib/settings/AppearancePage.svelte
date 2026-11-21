<script lang="ts">
  import { onMount } from 'svelte';
  import { appearanceChange, appearanceSettings, type AppearanceChange, type AppearancePage } from '../ipc';
  import { applyThemeCss } from '../theme';
  import { themeEpoch } from '../themeEpoch.svelte';

  const DENSITIES: { value: AppearancePage['density']; label: string }[] = [
    { value: 'automatic', label: 'Automatic' },
    { value: 'roomy', label: 'Roomy' },
    { value: 'compact', label: 'Compact' },
  ];

  let page: AppearancePage | null = $state(null);
  let error: string | null = $state(null);
  let busy = $state(false);

  async function change(c: AppearanceChange): Promise<void> {
    busy = true;
    try {
      const result = await appearanceChange(c);
      // The new variables first, then whoever read the old ink reads it again.
      applyThemeCss(document, result.css);
      themeEpoch.n++;
      page = result.page;
      error = null;
    } catch (e) {
      error = String(e);
      page = await appearanceSettings().catch(() => page);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    appearanceSettings().then(
      (p) => (page = p),
      (e: unknown) => (error = String(e)),
    );
  });
</script>

{#if page}
  <form onsubmit={(e) => e.preventDefault()} aria-busy={busy}>
    <label>
      <span>Theme</span>
      <select value={page.theme} disabled={busy} onchange={(e) => void change({ field: 'theme', value: e.currentTarget.value })}>
        {#each page.themes as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
    </label>
    <label>
      <span>Density</span>
      <select value={page.density} disabled={busy} onchange={(e) => void change({ field: 'density', value: e.currentTarget.value as AppearancePage['density'] })}>
        {#each DENSITIES as d (d.value)}<option value={d.value}>{d.label}</option>{/each}
      </select>
    </label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else if error}
  <p class="error" role="alert">{error}</p>
{:else}
  <p class="note">Loading appearance settings…</p>
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

  select {
    appearance: none;
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
