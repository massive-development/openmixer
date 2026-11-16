<script lang="ts">
  import { onMount } from 'svelte';
  import DeckHeader from './lib/deck/DeckHeader.svelte';
  import DeckPanel from './lib/deck/DeckPanel.svelte';
  import { DeckData } from './lib/decks.svelte';
  import { setText } from './lib/dom';
  import { benchStats, FrameRecorder } from './lib/bench';
  import {
    benchDone,
    control,
    isSmokePlay,
    loadControls,
    ready,
    runConfig,
    startStream,
    themeCss,
    type Controls,
  } from './lib/ipc';
  import Mixer from './lib/mixer/Mixer.svelte';
  import { firstSnapshot, type Snapshot } from './lib/packet';
  import { afterFirstPaint } from './lib/paint';
  import { applyThemeCss } from './lib/theme';
  import { ticker } from './lib/ticker';
  import Waveforms from './lib/waveform/Waveforms.svelte';

  // The slice shows two decks and their two channels.
  const DECKS = [0, 1];
  // A bench run lets the window settle after its first snapshot before it records.
  const BENCH_WARM_UP_MS = 3000;

  // The layout waits for the theme, so the first paint is already in the theme's colours.
  let themed = $state(false);
  // Raw: a snapshot is replaced whole 30 times a second, never mutated.
  let snapshot: Snapshot | null = $state.raw(null);
  let controls: Controls = $state.raw({});
  // The lanes' frame cap; the shell's `--ui-fps-cap` replaces the default.
  let fpsCap = $state(60);
  const decks = new DeckData();
  let hud: HTMLElement | undefined = $state();
  // Resolves once deck A reports a loaded track (the smoke run's round trip waits for it).
  let deckALoaded: (() => void) | null = null;

  function stream(onFirst: (s: Snapshot) => void): Promise<void> {
    return startStream((s) => {
      snapshot = s;
      if (deckALoaded && s.decks[0]?.loaded) {
        deckALoaded();
        deckALoaded = null;
      }
      onFirst(s);
    });
  }

  // A smoke run with a library proves the write path: deck A's play reaches the engine.
  async function smokePlay(): Promise<void> {
    await new Promise<void>((resolve) => {
      if (snapshot?.decks[0]?.loaded) resolve();
      else deckALoaded = resolve;
    });
    await control('deck0.play', 1);
  }

  const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

  // Records the drawn frames for `seconds` after a warm-up, then hands the result to the shell.
  async function bench(seconds: number): Promise<void> {
    const recorder = new FrameRecorder(seconds);
    await sleep(BENCH_WARM_UP_MS);
    ticker.record(recorder);
    await sleep(seconds * 1000);
    ticker.record(null);
    const playing = snapshot?.decks.filter((d) => d.playing).length ?? 0;
    const viewport: [number, number, number] = [window.innerWidth, window.innerHeight, window.devicePixelRatio || 1];
    await benchDone(benchStats(recorder, { benchSeconds: seconds, fpsCap, viewport, decksPlaying: playing }));
  }

  async function start(): Promise<void> {
    try {
      const config = await runConfig();
      fpsCap = config.fpsCap;
      applyThemeCss(document, await themeCss());
      controls = await loadControls().catch((error: unknown) => {
        console.error('openmixer: no control list', error);
        return {};
      });
      themed = true;
      await afterFirstPaint();
      const first = firstSnapshot(stream);
      // A smoke run proves the stream: it reports ready only after a valid packet arrives.
      if (config.smoke) {
        await first;
        if (await isSmokePlay()) await smokePlay();
      } else first.catch((error: unknown) => console.error('openmixer: the stream did not start', error));
      await ready();
      if (config.benchSeconds !== null) {
        await first;
        await bench(config.benchSeconds);
      }
    } catch (error) {
      console.error('openmixer: the shell did not start', error);
    }
  }

  onMount(() => {
    ticker.onHud((text) => setText(hud, text));
    const stop = decks.start(DECKS.length);
    void start();
    return () => void stop.then((unlisten) => unlisten());
  });
</script>

{#if themed}
  <main class="shell">
    <header class="top-bar">
      <span class="wordmark">OPENMIXER</span>
      <span class="hud num" bind:this={hud}></span>
    </header>
    <section class="deck-headers" aria-label="Deck headers">
      {#each DECKS as deck (deck)}
        <DeckHeader {deck} info={decks.infos[deck] ?? null} ink={decks.inks[deck] ?? null} {snapshot} />
      {/each}
    </section>
    <section class="waveforms" aria-label="Waveforms">
      <Waveforms {snapshot} {fpsCap} />
    </section>
    <section class="controls" aria-label="Controls">
      <DeckPanel deck={0} {controls} info={decks.infos[0] ?? null} ink={decks.inks[0] ?? null} {snapshot} />
      <Mixer {controls} {snapshot} />
      <DeckPanel deck={1} {controls} info={decks.infos[1] ?? null} ink={decks.inks[1] ?? null} {snapshot} mirrored />
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
    justify-content: space-between;
    padding: 0 var(--om-gap);
    background: var(--om-surface);
  }

  .wordmark {
    font-family: var(--om-font-display);
    font-weight: 700;
    font-size: 21px;
    letter-spacing: 0.14em;
    color: var(--om-text-primary);
  }

  .hud {
    color: var(--om-text-muted);
    font-size: var(--om-type-numeral);
  }

  .deck-headers,
  .controls {
    display: grid;
    gap: var(--om-border-width);
    min-height: 0;
  }

  .deck-headers {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .controls {
    grid-template-columns: minmax(0, 1fr) 492px minmax(0, 1fr);
  }

  .waveforms {
    min-height: 0;
    background: var(--om-well);
  }
</style>
