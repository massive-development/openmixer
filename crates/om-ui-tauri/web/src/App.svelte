<script lang="ts">
  import { onMount } from 'svelte';
  import { isSmoke, ready, startStream, themeCss } from './lib/ipc';
  import { firstSnapshot, type Snapshot } from './lib/packet';
  import { afterFirstPaint } from './lib/paint';
  import { applyThemeCss } from './lib/theme';
  import Waveforms from './lib/waveform/Waveforms.svelte';

  // The layout waits for the theme, so the first paint is already in the theme's colours.
  let themed = $state(false);
  // Raw: a snapshot is replaced whole 30 times a second, never mutated.
  let snapshot: Snapshot | null = $state.raw(null);

  function stream(onFirst: (s: Snapshot) => void): Promise<void> {
    return startStream((s) => {
      snapshot = s;
      onFirst(s);
    });
  }

  async function start(): Promise<void> {
    try {
      applyThemeCss(document, await themeCss());
      themed = true;
      await afterFirstPaint();
      const first = firstSnapshot(stream);
      // A smoke run proves the stream: it reports ready only after a valid packet arrives.
      if (await isSmoke()) await first;
      else first.catch((error: unknown) => console.error('openmixer: the stream did not start', error));
      await ready();
    } catch (error) {
      console.error('openmixer: the shell did not start', error);
    }
  }

  onMount(() => {
    void start();
  });
</script>

{#if themed}
  <main class="shell">
    <header class="top-bar"><span class="wordmark">OPENMIXER</span></header>
    <section class="deck-headers" aria-label="Deck headers">
      <div class="region"></div>
      <div class="region"></div>
    </section>
    <section class="waveforms" aria-label="Waveforms">
      <Waveforms {snapshot} />
    </section>
    <section class="controls" aria-label="Controls">
      <div class="region"></div>
      <div class="region"></div>
      <div class="region"></div>
    </section>
  </main>
{/if}

<style>
  /* The border colour shows only in the gaps, as the lines between regions. */
  .shell {
    display: grid;
    grid-template-rows: 44px 104px minmax(180px, 1fr) 372px;
    gap: var(--om-border-width);
    height: 100%;
    background: var(--om-border);
  }

  .top-bar {
    display: flex;
    align-items: center;
    padding-left: var(--om-gap);
    background: var(--om-surface);
  }

  .wordmark {
    font-family: var(--om-font-display);
    font-weight: 700;
    font-size: 21px;
    letter-spacing: 0.14em;
    color: var(--om-text-primary);
  }

  .deck-headers,
  .controls {
    display: grid;
    gap: var(--om-border-width);
  }

  .deck-headers {
    grid-template-columns: 1fr 1fr;
  }

  .controls {
    grid-template-columns: 1fr 492px 1fr;
  }

  .region {
    background: var(--om-surface);
  }

  .waveforms {
    min-height: 0;
    background: var(--om-well);
  }
</style>
