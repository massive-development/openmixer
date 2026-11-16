<script lang="ts">
  import { control, type ControlSpec } from '../ipc';
  import { Held, knobDelta } from './drag';
  import { toPosition, toValue } from './range';

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

  $effect(() => {
    void stamp;
    held.onSnapshot();
    shown = held.shown(value);
  });

  const angle = $derived(spec ? -135 + 270 * toPosition(shown, spec) : -135);

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
    held.begin(shown);
  }

  function onPointerMove(event: PointerEvent): void {
    if (!spec || !held.active) return;
    position = knobDelta(position, event.clientY - lastY);
    lastY = event.clientY;
    send(toValue(position, spec));
  }

  function onPointerUp(): void {
    held.end();
  }

  function onDoubleClick(): void {
    if (!spec) return;
    held.begin(spec.default);
    send(spec.default);
    held.end();
  }
</script>

<div class="cell" class:mirrored>
  <div
    class="knob"
    role="slider"
    tabindex="-1"
    aria-label={label}
    aria-valuenow={shown}
    aria-valuemin={spec?.min}
    aria-valuemax={spec?.max}
    style:--angle="{angle}deg"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    ondblclick={onDoubleClick}
  >
    <i></i>
  </div>
  {#if label}<span class="label">{label}</span>{/if}
</div>

<style>
  .cell {
    display: flex;
    align-items: center;
    gap: var(--om-density-space-2);
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
