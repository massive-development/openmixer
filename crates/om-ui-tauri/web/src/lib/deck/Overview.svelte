<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { controlName, Paced, positionText } from '../a11y';
  import { interpolate } from '../clock';
  import { PeakSync } from '../peaks';
  import { Held } from '../controls/drag';
  import { sliderKey } from '../controls/keys';
  import { setStyle } from '../dom';
  import { control, deckPeaks, waveformStyle, type DeckInfo, type StripInk } from '../ipc';
  import type { Snapshot } from '../packet';
  import { ticker } from '../ticker';
  import { seekValue } from '../waveform/view';
  import { levelOffset, overviewColumns, overviewLevel } from './overview';

  interface Props {
    deck: number;
    info: DeckInfo | null;
    ink: StripInk | null;
    snapshot: Snapshot | null;
  }

  let { deck, info, ink, snapshot }: Props = $props();

  let box: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let played: HTMLDivElement;
  let playhead: HTMLDivElement;
  let width = $state(0);
  let height = $state(0);
  let peaks: Uint8Array | null = $state.raw(null);
  let fill = 1;
  let minHalfPx = 0;
  // Without a played shade the style dims the played part to this brightness instead.
  let playedBrightness = $state(1);
  const held = new Held();
  let shownFrac = -1;
  // The position reaches assistive technology once a second, never per frame.
  const paced = new Paced(1000);
  let spokenSeconds = $state(0);

  const seekKey = $derived(`deck${deck}.seek`);
  const length = $derived(snapshot?.decks[deck]?.length ?? 0);
  const fileRate = $derived(snapshot?.decks[deck]?.fileRate ?? 0);
  const totalSeconds = $derived(length > 0 && fileRate > 0 ? length / fileRate : 0);
  const name = $derived(controlName(seekKey, ''));
  // A tick per set hot cue, at its share of the track.
  const cues = $derived(cueTicks(info, ink));

  function cueTicks(i: DeckInfo | null, k: StripInk | null): { left: string; colour: string }[] {
    if (!i || !k || !(i.numSamples > 0)) return [];
    return i.hotcues.flatMap((frame, n) =>
      frame === null ? [] : [{ left: `${(frame / i.numSamples) * 100}%`, colour: k.cues[n] ?? k.mainCue }],
    );
  }

  // Hot cue and title changes reload the info but keep its peaks generation, so only a new
  // waveform refetches and redraws; the cue ticks follow the info on their own.
  const peaksGeneration = $derived(info ? info.peaksGeneration : null);
  const sync = new PeakSync();

  $effect(() => {
    const generation = peaksGeneration;
    if (generation === null) return;
    untrack(() => follow(generation, (info?.levels.length ?? 0) > 0));
  });

  function follow(generation: number, hasPeaks: boolean): void {
    const fetch = sync.onInfo(generation, hasPeaks);
    // The old track's peaks are not drawn under a new waveform, nor for a deck without one.
    if (sync.showsOlder() || !hasPeaks) peaks = null;
    if (!fetch) return;
    deckPeaks(deck)
      .then((answer) => {
        if (sync.onPeaks(generation, answer.generation)) peaks = answer.bytes;
      })
      .catch((error: unknown) => {
        sync.onFailed(generation);
        console.error(`openmixer: deck ${deck} overview`, error);
      });
  }

  // Drawn once per track and size; the info is read as it is when the peaks arrive.
  $effect(() => {
    const [bytes, k, w, h] = [peaks, ink, width, height];
    untrack(() => draw(info, k, bytes, w, h));
  });

  $effect(() => {
    if (!snapshot) return;
    held.onSnapshot();
    ticker.wake(step);
  });

  // Drawn once per track and size: the Low-Band look, body then highs over it.
  function draw(i: DeckInfo | null, k: StripInk | null, bytes: Uint8Array | null, w: number, h: number): void {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    const g = canvas.getContext('2d');
    if (!g || !i || !k || !bytes) return;
    const level = overviewLevel(i.levels, i.numSamples, canvas.width);
    if (level < 0) return;
    const l = i.levels[level];
    const cols = overviewColumns(bytes, levelOffset(i.levels, level), l.len, l.samplesPerPoint, i.numSamples, canvas.width);
    band(g, cols.body, k.body, 1);
    band(g, cols.high, k.highs, k.highsAlpha);
  }

  function band(g: CanvasRenderingContext2D, heights: Float32Array, colour: string, alpha: number): void {
    const h = canvas.height;
    const half = (h / 2) * fill;
    const minHalf = minHalfPx * (window.devicePixelRatio || 1);
    g.globalAlpha = alpha;
    g.fillStyle = colour;
    g.beginPath();
    for (let x = 0; x < heights.length; x++) {
      if (heights[x] <= 0) continue;
      const y = Math.max(heights[x] * half, minHalf);
      g.rect(x, h / 2 - y, 1, 2 * y);
    }
    g.fill();
    g.globalAlpha = 1;
  }

  // One frame of the played shade and playhead; runs while the deck plays.
  function step(now: number): boolean {
    const snap = snapshot;
    const d = snap?.decks[deck];
    if (!snap || !d) return false;
    const frac = held.shown(d.loaded && d.length > 0 ? interpolate(d, snap, now) / d.length : 0);
    if (frac !== shownFrac) place(frac);
    speak(frac, now);
    return d.playing;
  }

  // Whole seconds, so a playing deck changes what is reported once a second at most.
  function speak(frac: number, now: number): void {
    const seconds = Math.floor(frac * totalSeconds);
    if (seconds !== spokenSeconds && paced.due(now)) spokenSeconds = seconds;
  }

  function place(frac: number): void {
    shownFrac = frac;
    setStyle(played, 'transform', `scaleX(${frac})`);
    setStyle(playhead, 'transform', `translate3d(${frac * width}px,0,0)`);
    ticker.drew();
  }

  function seekTo(frac: number): void {
    held.set(frac);
    place(frac);
    control(seekKey, seekValue(frac * length, length)).catch((error: unknown) =>
      console.error('openmixer: seek', error),
    );
  }

  function pointerFrac(event: PointerEvent): number {
    const r = box.getBoundingClientRect();
    return r.width > 0 ? Math.min(Math.max((event.clientX - r.left) / r.width, 0), 1) : 0;
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0 || !(length > 0)) return;
    box.setPointerCapture(event.pointerId);
    held.begin(Math.max(shownFrac, 0));
    seekTo(pointerFrac(event));
  }

  function onPointerMove(event: PointerEvent): void {
    if (held.active) seekTo(pointerFrac(event));
  }

  // Arrows seek by a hundredth of the track, Shift or the page keys by a tenth; no key resets it.
  function onKeyDown(event: KeyboardEvent): void {
    if (!(length > 0)) return;
    const next = sliderKey(event, Math.max(shownFrac, 0));
    if (next === null || next === 'reset') return;
    event.preventDefault();
    held.begin(next);
    seekTo(next);
    held.end();
    spokenSeconds = Math.floor(next * totalSeconds);
  }

  function onPointerUp(): void {
    held.end();
  }

  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      width = entry.contentRect.width;
      height = entry.contentRect.height;
      shownFrac = -1;
      ticker.wake(step);
    });
    observer.observe(box);
    waveformStyle()
      .then((style) => {
        fill = style.fill;
        minHalfPx = style.minHalfPx;
        playedBrightness = style.playedBrightness;
        draw(info, ink, peaks, width, height);
      })
      .catch((error: unknown) => console.error('openmixer: waveform style', error));
    return () => observer.disconnect();
  });
</script>

<div
  class="overview"
  class:idle={!(length > 0)}
  bind:this={box}
  style:background={ink?.well}
  data-control="deck{deck}.overview"
  role="slider"
  tabindex={length > 0 ? 0 : -1}
  aria-label={name}
  aria-disabled={length > 0 ? undefined : true}
  aria-valuemin={0}
  aria-valuemax={Math.floor(totalSeconds)}
  aria-valuenow={spokenSeconds}
  aria-valuetext={positionText(spokenSeconds, totalSeconds)}
  title={name}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onkeydown={onKeyDown}
>
  <canvas bind:this={canvas} aria-hidden="true"></canvas>
  <div
    class="played"
    bind:this={played}
    style:background={ink?.playedShade ?? ink?.well}
    style:opacity={ink?.playedShadeAlpha ?? 1 - playedBrightness}
  ></div>
  {#each cues as cue, n (n)}
    <div class="cue" style:left={cue.left} style:background={cue.colour}></div>
  {/each}
  <div
    class="playhead"
    bind:this={playhead}
    style:background={ink?.playhead}
    style:width={ink ? `${ink.playheadWidth}px` : null}
    style:margin-left={ink ? `${-ink.playheadWidth / 2}px` : null}
  ></div>
</div>

<style>
  .overview {
    position: relative;
    height: var(--om-deck-overview-height);
    overflow: hidden;
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-well);
    cursor: pointer;
    touch-action: none;
  }

  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  /* An empty deck has no position to show. */
  .idle {
    cursor: default;
  }

  .idle .played,
  .idle .playhead {
    visibility: hidden;
  }

  .played {
    position: absolute;
    inset: 0;
    transform: scaleX(0);
    transform-origin: left;
    will-change: transform;
    pointer-events: none;
  }

  .cue {
    position: absolute;
    top: 0;
    width: var(--om-cue-marker-width);
    height: 100%;
    pointer-events: none;
  }

  .playhead {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    will-change: transform;
    pointer-events: none;
  }
</style>
