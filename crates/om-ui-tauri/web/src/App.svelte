<script lang="ts">
  import { onMount } from 'svelte';
  import { deckName } from './lib/a11y';
  import DeckHeader from './lib/deck/DeckHeader.svelte';
  import DeckPanel from './lib/deck/DeckPanel.svelte';
  import AutomixBar from './lib/header/AutomixBar.svelte';
  import { Header } from './lib/header/header.svelte';
  import { isRecordKey, takesTyping } from './lib/header/keys';
  import RecordButton from './lib/header/RecordButton.svelte';
  import { DeckData } from './lib/decks.svelte';
  import { DECK_ATTRIBUTE, Loading } from './lib/loading.svelte';
  import { setText } from './lib/dom';
  import { benchStats, FrameRecorder } from './lib/bench';
  import { CONTROL_ATTRIBUTE, layoutMap } from './lib/drive';
  import {
    benchDone,
    control,
    isSmokePlay,
    loadControls,
    recordToggle,
    ready,
    runConfig,
    startStream,
    themeCss,
    uiLayout,
    type Controls,
  } from './lib/ipc';
  import Button from './lib/controls/Button.svelte';
  import Library from './lib/library/Library.svelte';
  import Mixer from './lib/mixer/Mixer.svelte';
  import { firstSnapshot, type Snapshot } from './lib/packet';
  import { afterFirstPaint } from './lib/paint';
  import { applyThemeCss } from './lib/theme';
  import { ticker } from './lib/ticker';
  import LaneSlider from './lib/waveform/LaneSlider.svelte';
  import { LaneSliders } from './lib/waveform/sliders.svelte';
  import Waveforms from './lib/waveform/Waveforms.svelte';

  // The slice shows two decks and their two channels.
  const DECKS = [0, 1];
  // The page's grid areas of deck A and deck B.
  const SIDES = ['a', 'b'];
  // A bench run lets the window settle after its first snapshot before it records.
  const BENCH_WARM_UP_MS = 3000;
  // A drive run measures its controls again this long after the last resize.
  const RESIZE_SETTLE_MS = 250;

  // The layout waits for the theme, so the first paint is already in the theme's colours.
  let themed = $state(false);
  // Raw: a snapshot is replaced whole 30 times a second, never mutated.
  let snapshot: Snapshot | null = $state.raw(null);
  let controls: Controls = $state.raw({});
  // The lanes' frame cap; the shell's `--ui-fps-cap` replaces the default.
  let fpsCap = $state(60);
  // The mock-device notice, when the run has one (`om_ui::labels`, by GPUI's rule).
  let banner: string | null = $state(null);
  const decks = new DeckData();
  // The open dialog, files dropped on a deck, and why a load failed.
  const loading = new Loading();
  // The top bar's Automix strip and Record button.
  const header = new Header();
  // Each lane's keyboard slider follows its deck's overview in the tab order.
  const sliders = new LaneSliders();
  // The library band under the decks; the top bar's Library button hides it and the stage grows.
  let libraryShown = $state(true);
  let hud: HTMLElement | undefined = $state();
  // The frame counter, only for a bench run or `--debug-panel`.
  let hudShown = $state(false);
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

  // `--ui-drive-out`: the controls' rectangles for an outside driver, once the layout settled.
  function sendLayout(): void {
    const map = layoutMap(document.querySelectorAll(`[${CONTROL_ATTRIBUTE}]`), window);
    uiLayout(map).catch((error: unknown) => console.error('openmixer: layout map', error));
  }

  // After the first snapshot (the lanes size themselves from it) and the fonts, then on resize.
  async function drive(first: Promise<Snapshot>): Promise<void> {
    await first;
    await document.fonts.ready;
    await afterFirstPaint();
    await afterFirstPaint();
    sendLayout();
    let timer: ReturnType<typeof setTimeout> | undefined;
    window.addEventListener('resize', () => {
      clearTimeout(timer);
      timer = setTimeout(sendLayout, RESIZE_SETTLE_MS);
    });
  }

  async function start(): Promise<void> {
    try {
      const config = await runConfig();
      fpsCap = config.fpsCap;
      banner = config.banner;
      hudShown = config.hud;
      if (config.hud) ticker.onHud((text) => setText(hud, text));
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
      if (config.drive) drive(first).catch((error: unknown) => console.error('openmixer: drive', error));
      await ready();
      if (config.benchSeconds !== null) {
        await first;
        await bench(config.benchSeconds);
      }
    } catch (error) {
      console.error('openmixer: the shell did not start', error);
    }
  }

  // `r` toggles recording, unless a text field takes the key (the Settings card, a search box).
  function recordKey(event: KeyboardEvent): void {
    if (!header.state || !isRecordKey(event) || takesTyping(event.target as HTMLElement | null)) return;
    event.preventDefault();
    void header.press(recordToggle);
  }

  onMount(() => {
    const stop = decks.start(DECKS.length);
    const stopLoading = loading.start(DECKS.length).catch((error: unknown) => {
      console.error('openmixer: file loading', error);
      return () => {};
    });
    void start();
    const stopHeader = header.start();
    return () => {
      stopHeader();
      void stop.then((unlisten) => unlisten());
      void stopLoading.then((unlisten) => unlisten());
    };
  });
</script>

<svelte:window onkeydown={recordKey} />

{#if themed}
  <!-- Source order is the reading and Tab order (deck A, mixer, deck B; a deck's header before its
       panel); the grid areas put each region in its row. -->
  <main class="shell" class:library-hidden={!libraryShown}>
    <header class="top-bar">
      <h1 class="wordmark">OPENMIXER</h1>
      {#if header.state}<AutomixBar bar={header.state.automix} {header} />{/if}
      <Button on={libraryShown} pressed={libraryShown} onpress={() => (libraryShown = !libraryShown)}>Library</Button>
      {#if banner}
        <p class="notice" role="status" title={banner}>{banner}</p>
      {/if}
      <!-- A frame counter for whoever tunes the window, not part of the instrument. -->
      {#if hudShown}<span class="hud num" bind:this={hud} aria-hidden="true"></span>{/if}
      {#if header.state}<RecordButton bar={header.state.record} {header} />{/if}
    </header>
    {#snippet deckRegions(deck: number)}
      <section
        class="region"
        style:grid-area="head-{SIDES[deck]}"
        aria-label="{deckName(deck)} track"
        {...{ [DECK_ATTRIBUTE]: deck }}
      >
        <DeckHeader
          {deck}
          info={decks.infos[deck] ?? null}
          ink={decks.inks[deck] ?? null}
          {snapshot}
          mirrored={deck === 1}
          dropping={loading.target === deck}
          note={loading.notes[deck] ?? null}
          onload={() => void loading.choose(deck).catch((error: unknown) => console.error('openmixer: open', error))}
        />
      </section>
      <LaneSlider {deck} {sliders} />
      <section
        class="region"
        style:grid-area="deck-{SIDES[deck]}"
        aria-label="{deckName(deck)} controls"
        {...{ [DECK_ATTRIBUTE]: deck }}
      >
        <DeckPanel
          {deck}
          {controls}
          info={decks.infos[deck] ?? null}
          ink={decks.inks[deck] ?? null}
          {snapshot}
          mirrored={deck === 1}
        />
      </section>
    {/snippet}
    {@render deckRegions(DECKS[0])}
    <section class="region mixer" aria-label="Mixer">
      <Mixer {controls} {snapshot} />
    </section>
    {@render deckRegions(DECKS[1])}
    <section class="waveforms" aria-label="Waveforms">
      <Waveforms {snapshot} {fpsCap} {sliders} />
    </section>
    {#if libraryShown}
      <section class="region library-band" aria-label="Library band">
        <Library />
      </section>
    {/if}
  </main>
{/if}

<style>
  /* The border colour shows only in the gaps, as the lines between regions. */
  .shell {
    display: grid;
    /* The two middle columns are the mixer (492 px with the line between them); a deck's header
       runs to the centre line over half of it. */
    grid-template-columns: minmax(0, 1fr) 246px 245px minmax(0, 1fr);
    grid-template-rows: 44px 104px minmax(150px, 0.6fr) 372px minmax(220px, 1fr);
    grid-template-areas:
      'top top top top'
      'head-a head-a head-b head-b'
      'wave wave wave wave'
      'deck-a mixer mixer deck-b'
      'lib lib lib lib';
    gap: var(--om-border-width);
    height: 100%;
    background: var(--om-border);
  }

  /* The library hidden (the top bar's button): the stage takes its height. */
  .shell.library-hidden {
    grid-template-rows: 44px 104px minmax(180px, 1fr) 372px;
  }

  .library-band {
    grid-area: lib;
  }

  .top-bar {
    grid-area: top;
    display: flex;
    align-items: center;
    justify-content: space-between;
    position: relative;
    padding: 0 var(--om-gap);
    background: var(--om-surface);
  }

  .wordmark {
    margin: 0;
    font-family: var(--om-font-display);
    font-weight: 700;
    font-size: var(--om-type-headline);
    letter-spacing: 0.14em;
    color: var(--om-text-primary);
  }

  /* Centred on the bar whatever sits either side of it; one line, cut with an ellipsis. */
  .notice {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    max-width: 50%;
    margin: 0;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    overflow: hidden;
    border: 1px solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font-size: var(--om-type-label);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .hud {
    color: var(--om-text-muted);
    font-size: var(--om-type-numeral);
  }

  /* One component fills each region. */
  .region {
    display: grid;
    min-width: 0;
    min-height: 0;
  }

  .mixer {
    grid-area: mixer;
  }

  .waveforms {
    grid-area: wave;
    min-height: 0;
    background: var(--om-well);
  }
</style>
