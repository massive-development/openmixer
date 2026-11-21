<script lang="ts">
  import { onMount } from 'svelte';
  import AppearancePage from './AppearancePage.svelte';
  import AudioPage from './AudioPage.svelte';
  import AutomixPage from './AutomixPage.svelte';
  import RecordingPage from './RecordingPage.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let dialog: HTMLElement | undefined = $state();

  // Pages added by later settings work join this list; the Audio page is the first.
  const PAGES = [
    { id: 'audio', label: 'Audio' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'automix', label: 'Automix' },
    { id: 'recording', label: 'Recording' },
  ];
  let page = $state('audio');

  function key(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      onclose();
    }
  }

  onMount(() => dialog?.focus());
</script>

<svelte:window onkeydown={key} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings" tabindex="-1" bind:this={dialog}>
    <header>
      <h2>Settings</h2>
      <button type="button" class="close" aria-label="Close settings" onclick={onclose}>Close</button>
    </header>
    <div class="body">
      <nav aria-label="Settings pages">
        {#each PAGES as p (p.id)}
          <button type="button" class:current={page === p.id} aria-current={page === p.id ? 'page' : undefined} onclick={() => (page = p.id)}>
            {p.label}
          </button>
        {/each}
      </nav>
      <section class="page">
        {#if page === 'audio'}<AudioPage />{:else if page === 'appearance'}<AppearancePage />{:else if page === 'automix'}<AutomixPage />{:else if page === 'recording'}<RecordingPage />{/if}
      </section>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--om-well) 70%, transparent);
    z-index: 10;
  }

  .dialog {
    width: min(720px, 92vw);
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface);
    color: var(--om-text-primary);
    outline: none;
  }

  .dialog:focus-visible {
    box-shadow: var(--om-focus-ring);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--om-gap);
    border-bottom: 1px solid var(--om-border);
  }

  h2 {
    margin: 0;
    font-family: var(--om-font-display);
    font-size: var(--om-type-headline);
    letter-spacing: 0.08em;
  }

  .body {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    min-height: 0;
    overflow: auto;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: var(--om-density-space-1);
    padding: var(--om-gap);
    border-right: 1px solid var(--om-border);
  }

  button {
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font: inherit;
    font-size: var(--om-type-label);
    text-align: left;
    cursor: pointer;
  }

  button:focus-visible {
    outline: none;
    box-shadow: var(--om-focus-ring);
  }

  button.current {
    color: var(--om-text-primary);
    border-color: var(--om-text-secondary);
  }

  .page {
    padding: var(--om-gap);
    min-width: 0;
  }
</style>
