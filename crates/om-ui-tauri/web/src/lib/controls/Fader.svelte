<script lang="ts">
  import { controlName, valueText } from '../a11y';
  import { control, type ControlSpec } from '../ipc';
  import { Held, faderValue } from './drag';
  import { sliderKey } from './keys';
  import { toPosition, toValue } from './range';

  interface Props {
    key: string;
    spec: ControlSpec | undefined;
    /** The snapshot's value, engine units. */
    value: number;
    /** Changes with every snapshot, so a held value knows when the engine has caught up. */
    stamp: number;
    /** Vertical faders read 1 at the top; the crossfader is horizontal. */
    horizontal?: boolean;
    /** Whether a double-click, or Delete, restores the default. */
    resettable?: boolean;
  }

  let { key, spec, value, stamp, horizontal = false, resettable = true }: Props = $props();

  const held = new Held();
  let shown = $state(0);
  let thumb: HTMLDivElement;

  $effect(() => {
    void stamp;
    held.onSnapshot();
    shown = held.shown(value);
  });

  const position = $derived(spec ? toPosition(shown, spec) : 0);
  const name = $derived(controlName(key, ''));
  const text = $derived(valueText(key, shown, spec));

  function send(v: number): void {
    held.set(v);
    shown = v;
    control(key, v).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }

  // The thumb's centre travels between half a thumb in from each end.
  function positionAt(event: PointerEvent, track: HTMLElement): number {
    const r = track.getBoundingClientRect();
    if (horizontal) {
      const half = thumb.offsetWidth / 2;
      return faderValue(event.clientX, r.left + half, r.width - 2 * half, false);
    }
    const half = thumb.offsetHeight / 2;
    return faderValue(event.clientY, r.top + half, r.height - 2 * half, true);
  }

  function onPointerDown(event: PointerEvent): void {
    if (!spec || event.button !== 0) return;
    const track = event.currentTarget as HTMLElement;
    track.setPointerCapture(event.pointerId);
    held.begin(shown);
    send(toValue(positionAt(event, track), spec));
  }

  function onPointerMove(event: PointerEvent): void {
    if (!spec || !held.active) return;
    send(toValue(positionAt(event, event.currentTarget as HTMLElement), spec));
  }

  function onPointerUp(): void {
    held.end();
  }

  // One whole move: held as a drag is, so the thumb does not flick back before the engine answers.
  function setTo(v: number): void {
    held.begin(v);
    send(v);
    held.end();
  }

  function onDoubleClick(): void {
    if (spec && resettable) setTo(spec.default);
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (!spec) return;
    const next = sliderKey(event, position);
    if (next === null || (next === 'reset' && !resettable)) return;
    event.preventDefault();
    setTo(next === 'reset' ? spec.default : toValue(next, spec));
  }
</script>

<div
  class="fader"
  class:horizontal
  role="slider"
  tabindex="0"
  aria-label={name}
  aria-valuenow={shown}
  aria-valuemin={spec?.min}
  aria-valuemax={spec?.max}
  aria-valuetext={text}
  aria-orientation={horizontal ? 'horizontal' : 'vertical'}
  title="{name}: {text}"
  style:--pos={position}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={onDoubleClick}
  onkeydown={onKeyDown}
>
  <div class="slot"></div>
  {#if !horizontal}<div class="ticks"></div>{/if}
  <div class="thumb" bind:this={thumb}></div>
</div>

<style>
  .fader {
    --thumb: var(--om-density-control-height);
    position: relative;
    width: var(--om-mixer-fader-width);
    height: 100%;
    min-height: 0;
    cursor: ns-resize;
    touch-action: none;
  }

  .horizontal {
    width: 100%;
    height: var(--om-mixer-crossfader-height);
    cursor: ew-resize;
  }

  /* The recessed slot the thumb runs in, between the thumb's centre at each end. */
  .slot {
    position: absolute;
    left: 50%;
    top: calc(var(--thumb) / 2);
    bottom: calc(var(--thumb) / 2);
    width: 2px;
    margin-left: -1px;
    background: var(--om-well);
    box-shadow: 0 0 0 var(--om-border-width) var(--om-border);
  }

  .horizontal .slot {
    top: 50%;
    bottom: auto;
    left: calc(var(--thumb) / 2);
    right: calc(var(--thumb) / 2);
    width: auto;
    height: 2px;
    margin: -1px 0 0;
  }

  /* Eight equal steps of tick marks along both edges of the travel. */
  .ticks {
    position: absolute;
    top: calc(var(--thumb) / 2);
    bottom: calc(var(--thumb) / 2);
    left: 0;
    right: 0;
    pointer-events: none;
    background:
      repeating-linear-gradient(to bottom, var(--om-border) 0 1px, transparent 1px 12.5%) left / var(--om-inner-gap)
        calc(100% - 1px) no-repeat,
      repeating-linear-gradient(to bottom, var(--om-border) 0 1px, transparent 1px 12.5%) right / var(--om-inner-gap)
        calc(100% - 1px) no-repeat;
  }

  .thumb {
    position: absolute;
    left: var(--om-inner-gap);
    right: var(--om-inner-gap);
    top: calc((1 - var(--pos)) * (100% - var(--thumb)));
    height: var(--thumb);
    border: var(--om-border-width) solid var(--om-border-strong);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    box-shadow: 0 var(--om-shadow-offset-y) var(--om-shadow-blur) var(--om-shadow);
  }

  .horizontal .thumb {
    top: var(--om-inner-gap);
    bottom: var(--om-inner-gap);
    left: calc(var(--pos) * (100% - var(--thumb)));
    right: auto;
    width: var(--thumb);
    height: auto;
  }

  /* The centre line across the thumb. */
  .thumb::after {
    content: '';
    position: absolute;
    left: var(--om-inset-small);
    right: var(--om-inset-small);
    top: 50%;
    height: 2px;
    margin-top: -1px;
    background: var(--om-text-primary);
  }

  .horizontal .thumb::after {
    left: 50%;
    right: auto;
    top: var(--om-inset-small);
    bottom: var(--om-inset-small);
    width: 2px;
    height: auto;
    margin: 0 0 0 -1px;
  }
</style>
