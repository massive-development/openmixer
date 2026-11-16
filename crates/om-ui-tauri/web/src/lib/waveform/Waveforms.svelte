<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { interpolate } from '../clock';
  import {
    control,
    deckInfo,
    deckPeaks,
    onDeckChanged,
    stripInk,
    waveformStyle,
    type DeckInfo,
    type StripInk,
  } from '../ipc';
  import type { DeckFrame, Snapshot } from '../packet';
  import { WaveGl, laneInk, type LaneInk, type LanePaint, type PeakTexture } from './gl';
  import { Scrub } from './scrub';
  import {
    BEATS_PER_BAR,
    MIN_BEAT_SPACING_PX,
    barStep,
    firstLabelledBar,
    frameToX,
    laneRect,
    pickLevel,
    seekValue,
    shouldDraw,
    viewScale,
    zoomBy,
    type LaneRect,
  } from './view';

  interface Props {
    snapshot: Snapshot | null;
    visibleSeconds?: number;
    fpsCap?: number;
  }

  /** One deck's lane: its track, peaks and ink, the drag, and the pooled overlay elements. */
  interface Lane {
    info: DeckInfo | null;
    peaks: PeakTexture | null;
    ink: LaneInk | null;
    dto: StripInk | null;
    request: number;
    scrub: Scrub;
    paint: LanePaint | null;
    seekKey: string;
    shown: number;
    spp: number;
    beatLen: number;
    bars: HTMLElement[];
    barN: Int32Array;
    barX: Float64Array;
    barOn: Uint8Array;
    flags: HTMLElement[];
    flagX: Float64Array;
    flagOn: Uint8Array;
  }

  // Enough bar numbers for a 3000 px lane at the 48 px minimum spacing.
  const BAR_SLOTS = Array.from({ length: 64 }, (_, j) => j);
  // Hot cues 1..8, as `deck_info` lists them.
  const CUE_SLOTS = Array.from({ length: 8 }, (_, j) => j);

  let { snapshot, visibleSeconds = 8, fpsCap = 60 }: Props = $props();

  let root: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  // Mutated in place (never reassigned) so the template binds into the same objects.
  const lanes: Lane[] = [];
  let boxes: LaneRect[] = $state.raw([]);
  let inks: (StripInk | null)[] = $state.raw([]);

  let wgl: WaveGl | null = null;
  let latest: Snapshot | null = null;
  let seconds = 8;
  let fill = 1;
  let playedBrightness = 1;
  let cssWidth = 0;
  let cssHeight = 0;
  let devWidth = 0;
  let devHeight = 0;
  let scale = 1;
  let lastDrawn = -Infinity;
  let dirty = true;
  let dragging = -1;
  let positions = new Float64Array(0);

  $effect(() => {
    const snap = snapshot;
    if (snap) untrack(() => receive(snap));
  });

  function receive(snap: Snapshot): void {
    latest = snap;
    if (snap.decks.length !== lanes.length) setLaneCount(snap.decks.length);
    for (const lane of lanes) lane.scrub.onSnapshot();
  }

  function newLane(deck: number): Lane {
    return {
      info: null,
      peaks: null,
      ink: null,
      dto: null,
      request: 0,
      scrub: new Scrub(),
      paint: null,
      seekKey: `deck${deck}.seek`,
      shown: NaN,
      spp: NaN,
      beatLen: 0,
      bars: [],
      barN: new Int32Array(BAR_SLOTS.length).fill(-1),
      barX: new Float64Array(BAR_SLOTS.length).fill(NaN),
      barOn: new Uint8Array(BAR_SLOTS.length),
      flags: [],
      flagX: new Float64Array(CUE_SLOTS.length).fill(NaN),
      flagOn: new Uint8Array(CUE_SLOTS.length),
    };
  }

  function setLaneCount(count: number): void {
    for (const lane of lanes.slice(count)) wgl?.release(lane.peaks);
    const first = Math.min(lanes.length, count);
    lanes.length = count;
    for (let i = first; i < count; i++) lanes[i] = newLane(i);
    positions = new Float64Array(count);
    inks = lanes.map((lane) => lane.dto);
    for (let i = first; i < count; i++) {
      void loadTrack(i);
      void loadInk(i);
    }
    resizeLanes();
  }

  async function loadTrack(deck: number): Promise<void> {
    const lane = lanes[deck];
    const request = ++lane.request;
    try {
      const [info, bytes] = await Promise.all([deckInfo(deck), deckPeaks(deck)]);
      if (request !== lane.request || lanes[deck] !== lane) return;
      wgl?.release(lane.peaks);
      lane.peaks = wgl && info.levels.length > 0 ? wgl.upload(info.levels, bytes) : null;
      lane.info = info;
      dirty = true;
    } catch (error) {
      console.error(`openmixer: deck ${deck} track`, error);
    }
  }

  async function loadInk(deck: number): Promise<void> {
    try {
      const dto = await stripInk(deck);
      const lane = lanes[deck];
      if (!lane) return;
      lane.dto = dto;
      lane.ink = laneInk(dto);
      inks = lanes.map((l) => l.dto);
      dirty = true;
    } catch (error) {
      console.error(`openmixer: deck ${deck} ink`, error);
    }
  }

  // The device-pixel backing store and the lanes' boxes follow the element's size and scale.
  function measure(): boolean {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(cssWidth * dpr);
    const h = Math.round(cssHeight * dpr);
    if (w !== devWidth || h !== devHeight || dpr !== scale) {
      scale = dpr;
      devWidth = w;
      devHeight = h;
      canvas.width = w;
      canvas.height = h;
      resizeLanes();
    }
    return devWidth > 0 && devHeight > 0 && lanes.length > 0;
  }

  function separatorPx(): number {
    return Math.max(1, Math.round(scale));
  }

  function resizeLanes(): void {
    const sep = separatorPx();
    boxes = lanes.map((_, i) => {
      const r = laneRect(i, lanes.length, devHeight, sep);
      return { top: r.top / scale, height: r.height / scale };
    });
    dirty = true;
  }

  function sampleRateOf(lane: Lane, deck: DeckFrame): number {
    return lane.info && lane.info.sampleRate > 0 ? lane.info.sampleRate : deck.fileRate;
  }

  // Reads every lane's position and zoom; true when any differs from the last drawn frame.
  function moved(snap: Snapshot, now: number): boolean {
    let changed = dirty;
    for (let i = 0; i < lanes.length; i++) {
      const lane = lanes[i];
      const deck = snap.decks[i];
      const pos = lane.scrub.held ?? interpolate(deck, snap, now);
      const spp = viewScale(seconds, sampleRateOf(lane, deck), devWidth);
      positions[i] = pos;
      if (pos !== lane.shown || spp !== lane.spp) changed = true;
      lane.spp = spp;
    }
    return changed;
  }

  function frame(now: number): void {
    raf = requestAnimationFrame(frame);
    const snap = latest;
    if (!snap || !wgl || !shouldDraw(now, lastDrawn, fpsCap) || !measure()) return;
    if (snap.decks.length !== lanes.length || !moved(snap, performance.now())) return;
    dirty = false;
    lastDrawn = now;
    for (let i = 0; i < lanes.length; i++) drawLane(i, snap.decks[i]);
  }

  function drawLane(i: number, deck: DeckFrame): void {
    const lane = lanes[i];
    lane.shown = positions[i];
    sendSeek(lane, deck);
    if (!wgl || !lane.ink) return;
    const paint = fillPaint(lane, lane.ink, i, deck);
    wgl.draw(paint);
    placeBars(lane, deck);
    placeFlags(lane);
  }

  function sendSeek(lane: Lane, deck: DeckFrame): void {
    const frame = lane.scrub.takeSend();
    if (frame !== null) control(lane.seekKey, seekValue(frame, deck.length)).catch(seekFailed);
  }

  function seekFailed(error: unknown): void {
    console.error('openmixer: seek', error);
  }

  function fillPaint(lane: Lane, ink: LaneInk, i: number, deck: DeckFrame): LanePaint {
    const rect = laneRect(i, lanes.length, devHeight, separatorPx());
    const p = (lane.paint ??= blankPaint(ink));
    p.ink = ink;
    p.originY = devHeight - rect.top - rect.height;
    p.width = devWidth;
    p.height = rect.height;
    p.centre = lane.shown;
    p.spp = lane.spp;
    p.length = deck.loaded ? deck.length : 0;
    p.scale = Math.max(1, Math.round(scale));
    p.fill = fill;
    p.playedBrightness = playedBrightness;
    fillLevel(p, lane);
    fillGrid(p, lane, deck);
    return p;
  }

  function fillLevel(p: LanePaint, lane: Lane): void {
    const levels = lane.info?.levels ?? [];
    const level = lane.peaks ? pickLevel(levels, lane.spp) : -1;
    p.peaks = lane.peaks;
    p.level = level;
    p.count = level >= 0 ? levels[level].len : 0;
    p.spPoint = level >= 0 ? levels[level].samplesPerPoint : 0;
  }

  // The grid needs a tempo and beat 0; without either it is left out.
  function fillGrid(p: LanePaint, lane: Lane, deck: DeckFrame): void {
    const bpm = lane.info?.bpm ?? null;
    const first = lane.info?.firstBeatFrame ?? null;
    const rate = sampleRateOf(lane, deck);
    lane.beatLen = bpm !== null && bpm > 0 && first !== null && rate > 0 ? (60 / bpm) * rate : 0;
    p.beatLen = lane.beatLen;
    p.firstBeat = first ?? 0;
    p.beatLines = lane.spp > 0 && lane.beatLen / lane.spp >= MIN_BEAT_SPACING_PX;
  }

  function blankPaint(ink: LaneInk): LanePaint {
    return {
      originY: 0, width: 0, height: 0, centre: 0, spp: 0, length: 0, peaks: null, level: -1,
      count: 0, spPoint: 0, beatLen: 0, firstBeat: 0, beatLines: false, scale: 1, ink, fill: 1,
      playedBrightness: 1,
    };
  }

  // Bar numbers ride the bar lines (om-ui-gpui `zoom`: 1 px right of the line, at the bottom).
  function placeBars(lane: Lane, deck: DeckFrame): void {
    const first = lane.info?.firstBeatFrame ?? null;
    const dto = lane.dto;
    if (!dto || first === null || lane.beatLen <= 0 || lane.spp <= 0) return hideAll(lane.bars, lane.barOn);
    const perBar = lane.beatLen * BEATS_PER_BAR;
    const step = barStep(perBar / lane.spp / scale, dto.barNumberMinSpacingPx);
    const left = lane.shown - (devWidth / 2 + dto.barNumberMinSpacingPx * scale) * lane.spp;
    const n0 = firstLabelledBar(left, first, perBar, step);
    for (let j = 0; j < lane.bars.length; j++) {
      const n = n0 + j * step;
      const at = first + n * perBar;
      const x = frameToX(at, lane.shown, lane.spp, devWidth) / scale + 1;
      const on = at >= 0 && at < deck.length && x < cssWidth;
      show(lane.bars[j], lane.barOn, j, on);
      if (on) place(lane.bars[j], lane.barX, j, x, lane.barN, n + 1);
    }
  }

  // A flag shows while its line or tab touches the lane (`om_ui::waveform::cue_flags`).
  function placeFlags(lane: Lane): void {
    const cues = lane.info?.hotcues ?? [];
    const width = lane.dto?.cueFlagWidth ?? 0;
    for (let j = 0; j < lane.flags.length; j++) {
      const cue = cues[j] ?? null;
      const x = cue === null || lane.spp <= 0 ? NaN : frameToX(cue, lane.shown, lane.spp, devWidth) / scale;
      const on = x + width >= 0 && x <= cssWidth;
      show(lane.flags[j], lane.flagOn, j, on);
      if (on) place(lane.flags[j], lane.flagX, j, x, null, 0);
    }
  }

  function show(el: HTMLElement | undefined, shown: Uint8Array, j: number, on: boolean): void {
    if (!el || shown[j] === (on ? 1 : 0)) return;
    shown[j] = on ? 1 : 0;
    el.style.display = on ? 'block' : '';
  }

  function place(el: HTMLElement | undefined, xs: Float64Array, j: number, x: number, ns: Int32Array | null, n: number): void {
    if (!el) return;
    if (xs[j] !== x) {
      xs[j] = x;
      el.style.transform = `translate3d(${x}px,0,0)`;
    }
    if (ns && ns[j] !== n) {
      ns[j] = n;
      el.textContent = String(n);
    }
  }

  function hideAll(els: HTMLElement[], shown: Uint8Array): void {
    for (let j = 0; j < els.length; j++) show(els[j], shown, j, false);
  }

  function onWheel(event: WheelEvent): void {
    event.preventDefault();
    seconds = zoomBy(seconds, event.deltaY);
  }

  function laneAt(y: number): number {
    return boxes.findIndex((box) => y >= box.top && y < box.top + box.height);
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    const i = laneAt(event.clientY - root.getBoundingClientRect().top);
    const lane = lanes[i];
    if (!lane || Number.isNaN(lane.shown)) return;
    root.setPointerCapture(event.pointerId);
    dragging = i;
    lane.scrub.begin(lane.shown, event.clientX * scale);
  }

  function onPointerMove(event: PointerEvent): void {
    const lane = lanes[dragging];
    const deck = latest?.decks[dragging];
    if (lane && deck) lane.scrub.move(event.clientX * scale, lane.spp, deck.length);
  }

  function onPointerUp(): void {
    lanes[dragging]?.scrub.end();
    dragging = -1;
  }

  let raf = 0;

  onMount(() => {
    seconds = visibleSeconds;
    wgl = WaveGl.create(canvas);
    const observer = new ResizeObserver(([entry]) => {
      cssWidth = entry.contentRect.width;
      cssHeight = entry.contentRect.height;
    });
    observer.observe(root);
    const unlisten = onDeckChanged(({ deck }) => {
      if (deck < lanes.length) void loadTrack(deck);
    });
    waveformStyle()
      .then((style) => {
        fill = style.fill;
        playedBrightness = style.playedBrightness;
        dirty = true;
      })
      .catch((error: unknown) => console.error('openmixer: waveform style', error));
    root.addEventListener('wheel', onWheel, { passive: false });
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      root.removeEventListener('wheel', onWheel);
      void unlisten.then((stop) => stop());
      for (const lane of lanes) wgl?.release(lane.peaks);
    };
  });
</script>

<div
  class="waveforms"
  bind:this={root}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  role="presentation"
>
  <canvas bind:this={canvas}></canvas>
  {#each boxes as box, i (i)}
    {#if i > 0}
      <div class="separator" style:top="{box.top}px" style:transform="translateY(-100%)"></div>
    {/if}
    <div class="lane" style:top="{box.top}px" style:height="{box.height}px">
      {#each BAR_SLOTS as j (j)}
        <span
          class="bar num"
          bind:this={lanes[i].bars[j]}
          style:font-size={inks[i] ? `${inks[i]?.barNumberSize}px` : null}
        ></span>
      {/each}
      {#each CUE_SLOTS as j (j)}
        <div class="flag" bind:this={lanes[i].flags[j]}>
          <div class="guide" style:background={inks[i]?.cueGuide} style:opacity={inks[i]?.cueGuideAlpha}></div>
          <span
            class="tab num"
            style:background={inks[i]?.cues[j]}
            style:width={inks[i] ? `${inks[i]?.cueFlagWidth}px` : null}
            style:height={inks[i] ? `${inks[i]?.cueFlagHeight}px` : null}>{j + 1}</span
          >
        </div>
      {/each}
      <span class="letter" style:color="var(--om-accent-{i})">{String.fromCharCode(65 + i)}</span>
    </div>
  {/each}
</div>

<style>
  .waveforms {
    position: relative;
    height: 100%;
    overflow: hidden;
    background: var(--om-well);
    touch-action: none;
    user-select: none;
  }

  canvas {
    position: absolute;
    top: 0;
    left: 0;
    display: block;
    width: 100%;
    height: 100%;
  }

  .separator {
    position: absolute;
    left: 0;
    width: 100%;
    height: 1px;
    background: var(--om-border);
  }

  /* Overlays only: the pointer goes to the region, which picks the lane. */
  .lane {
    position: absolute;
    left: 0;
    width: 100%;
    overflow: hidden;
    pointer-events: none;
  }

  .letter {
    position: absolute;
    top: var(--om-inner-gap);
    left: var(--om-inner-gap);
    font-family: var(--om-font-display);
    font-size: 13px;
  }

  .bar {
    position: absolute;
    left: 0;
    bottom: var(--om-density-space-2);
    display: none;
    color: var(--om-text-secondary);
  }

  .flag {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    display: none;
  }

  .guide {
    position: absolute;
    top: 0;
    left: 0;
    width: var(--om-cue-marker-width);
    height: 100%;
  }

  .tab {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--om-type-numeral);
    color: var(--om-on-accent);
  }
</style>
