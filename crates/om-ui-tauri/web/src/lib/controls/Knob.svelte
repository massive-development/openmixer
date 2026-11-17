<script lang="ts">
  import { controlName, valueText } from '../a11y';
  import { control, type ControlSpec } from '../ipc';
  import { Held, knobDelta } from './drag';
  import { keyValue } from './keys';
  import { toPosition, toValue } from './range';
  import { dragSnap } from './snap';

  interface Props {
    key: string;
    spec: ControlSpec | undefined;
    /** The snapshot's value, engine units. */
    value: number;
    /** Changes with every snapshot, so a held value knows when the engine has caught up. */
    stamp: number;
    label?: string;
    /** Label on the knob's left (deck B's strip mirrors deck A's). */
    mirrored?: boolean;
  }

  let { key, spec, value, stamp, label = '', mirrored = false }: Props = $props();

  const held = new Held();
  let shown = $state(0);
  let position = 0;
  let lastY = 0;
  // The value and Shift at the press: GPUI pulls to the detent only once the value moves, and
  // Shift held at the press makes the whole drag fine: a tenth as far, and no pull.
  let anchor = 0;
  let fine = false;

  $effect(() => {
    void stamp;
    held.onSnapshot();
    shown = held.shown(value);
  });

  const angle = $derived(spec ? sweepAngle(toPosition(shown, spec)) : -135);
  // The detent mark sits where the detent value does: 12 o'clock unless a taper moves it.
  const markAngle = $derived(spec && spec.detent !== null ? sweepAngle(toPosition(spec.detent, spec)) : null);
  const name = $derived(controlName(key, label));
  const text = $derived(valueText(key, shown, spec));

  function sweepAngle(position: number): number {
    return -135 + 270 * position;
  }

  function send(v: number): void {
    held.set(v);
    shown = v;
    control(key, v).catch((error: unknown) => console.error(`openmixer: ${key}`, error));
  }

  function onPointerDown(event: PointerEvent): void {
    if (!spec || event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    position = toPosition(shown, spec);
    lastY = event.clientY;
    anchor = shown;
    fine = event.shiftKey;
    held.begin(shown);
  }

  function onPointerMove(event: PointerEvent): void {
    if (!spec || !held.active) return;
    position = knobDelta(position, event.clientY - lastY, fine);
    lastY = event.clientY;
    send(dragSnap(toValue(position, spec), anchor, spec.snap, fine));
  }

  function onPointerUp(): void {
    held.end();
  }

  // One whole move: held as a drag is, so the knob does not flick back before the engine answers.
  function setTo(v: number): void {
    held.begin(v);
    send(v);
    held.end();
  }

  function onDoubleClick(): void {
    if (spec) setTo(spec.default);
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (!spec) return;
    const next = keyValue(event, shown, spec);
    if (next === null) return;
    event.preventDefault();
    setTo(next);
  }
</script>

<div class="cell" class:mirrored>
  <div
    class="knob"
    role="slider"
    tabindex="0"
    aria-label={name}
    aria-valuenow={shown}
    aria-valuemin={spec?.min}
    aria-valuemax={spec?.max}
    aria-valuetext={text}
    title="{name}: {text}"
    style:--angle="{angle}deg"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    ondblclick={onDoubleClick}
    onkeydown={onKeyDown}
  >
    {#if markAngle !== null}<b class="mark" style:--mark="{markAngle}deg"></b>{/if}
    <i></i>
  </div>
  <!-- The slider's name already says it. -->
  {#if label}<span class="label" aria-hidden="true">{label}</span>{/if}
</div>

<style>
  /* The gap clears the focus ring, so the ring never crosses the label. */
  .cell {
    display: flex;
    align-items: center;
    gap: var(--om-density-space-3);
    min-width: 0;
  }

  .mirrored {
    flex-direction: row-reverse;
  }

  .knob {
    position: relative;
    flex: none;
    width: var(--om-density-knob-size);
    height: var(--om-density-knob-size);
    border: var(--om-border-width) solid var(--om-border-strong);
    border-radius: 50%;
    background: var(--om-surface-raised);
    box-shadow: 0 var(--om-shadow-offset-y) var(--om-shadow-blur) var(--om-shadow);
    cursor: ns-resize;
    touch-action: none;
  }

  /* A compact knob is smaller than a pointer target: the catch area grows to the theme's minimum. */
  .knob::after {
    content: '';
    position: absolute;
    inset: min(0px, (var(--om-density-knob-size) - var(--om-min-pointer-target)) / 2 - var(--om-border-width));
    border-radius: 50%;
  }

  /* The mark beside a knob with a detent: where the pointer rests on it. */
  .mark {
    position: absolute;
    inset: 0;
    transform: rotate(var(--mark));
    pointer-events: none;
  }

  .mark::before {
    content: '';
    position: absolute;
    left: 50%;
    bottom: calc(100% + var(--om-inset-small));
    width: var(--om-border-width);
    height: var(--om-inset-large);
    margin-left: calc(var(--om-border-width) / -2);
    background: var(--om-border-strong);
  }

  .knob i {
    position: absolute;
    inset: 0;
    transform: rotate(var(--angle));
  }

  /* The pointer line from the rim towards the centre. */
  .knob i::after {
    content: '';
    position: absolute;
    left: 50%;
    top: var(--om-inset-small);
    width: 2px;
    height: 30%;
    margin-left: -1px;
    background: var(--om-text-primary);
  }

  .label {
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
    white-space: nowrap;
  }
</style>
