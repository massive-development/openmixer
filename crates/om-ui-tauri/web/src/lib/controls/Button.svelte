<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    on?: boolean;
    /** A deck or cue colour: the on-state is tinted in it (border and fill) instead of neutral. */
    tint?: string | null;
    /** The playing face: the tint as a solid fill under ground-colour text. */
    solid?: boolean;
    tall?: boolean;
    /** The accessible name, where the printed word alone does not say which deck it is. */
    label?: string;
    /** A toggle's state for assistive technology; leave out for a trigger or a button whose word changes. */
    pressed?: boolean;
    disabled?: boolean;
    title?: string;
    onpress: (event: MouseEvent) => void;
    onkeydown?: (event: KeyboardEvent) => void;
    children: Snippet;
  }

  let {
    on = false,
    tint = null,
    solid = false,
    tall = false,
    label,
    pressed,
    disabled = false,
    title,
    onpress,
    onkeydown,
    children,
  }: Props = $props();
</script>

<button
  type="button"
  class="btn"
  class:on
  class:tinted={on && !solid && tint !== null}
  class:solid={on && solid && tint !== null}
  class:lit={pressed === true}
  class:tall
  style:--tint={tint}
  aria-label={label}
  aria-pressed={pressed}
  {disabled}
  {title}
  onclick={onpress}
  {onkeydown}
>
  {@render children()}
</button>

<style>
  .btn {
    position: relative;
    min-width: var(--om-min-pointer-target);
    min-height: var(--om-min-pointer-target);
    padding: 0 var(--om-inner-gap);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    color: var(--om-text-secondary);
    font-family: var(--om-font-body);
    font-size: var(--om-type-label);
    font-weight: 500;
    text-transform: uppercase;
    cursor: pointer;
    transition:
      background-color var(--om-motion-fast-ms),
      border-color var(--om-motion-fast-ms),
      color var(--om-motion-fast-ms);
  }

  .tall {
    min-height: var(--om-min-target);
    font-size: var(--om-type-body);
  }

  .btn:hover {
    background: var(--om-surface-hover);
    color: var(--om-text-primary);
  }

  .on {
    color: var(--om-text-primary);
    border-color: var(--om-border-strong);
    background: var(--om-surface-hover);
  }

  /* A toggle that is on carries a bar along its foot, so the state does not rest on colour. */
  .lit {
    box-shadow: inset 0 calc(-1 * var(--om-inset-small)) 0 currentColor;
  }

  /* The tint is two layers at the theme's deck alphas, so any theme colour works without mixing.
     The words stay in the text colour: a hue on its own tint falls under 4.5:1. */
  .tinted {
    border-color: transparent;
    background: var(--om-surface-raised);
    isolation: isolate;
  }

  .tinted::before,
  .tinted::after {
    content: '';
    position: absolute;
    inset: calc(-1 * var(--om-border-width));
    border-radius: inherit;
    pointer-events: none;
    transition: opacity var(--om-motion-fast-ms);
  }

  /* Under the words, over the face. */
  .tinted::before {
    z-index: -1;
    background: var(--tint);
    opacity: var(--om-deck-tint-alpha);
  }

  .tinted::after {
    border: var(--om-border-width) solid var(--tint);
    opacity: var(--om-deck-border-alpha);
  }

  .tinted:hover::before {
    opacity: var(--om-deck-tint-hover-alpha);
  }

  .tinted:hover::after {
    opacity: var(--om-deck-border-hover-alpha);
  }

  .solid,
  .solid:hover {
    border-color: var(--tint);
    background: var(--tint);
    color: var(--om-on-accent);
  }

  /* Pressed: the face sinks to the well. */
  .btn:active {
    background: var(--om-well);
    box-shadow: inset 0 var(--om-shadow-offset-y) var(--om-shadow-blur) var(--om-shadow);
  }

  .solid:active {
    color: var(--tint);
  }

  /* Disabled: the panel's tone and a dashed edge; it takes no pointer, so no hover or press. */
  .btn:disabled {
    border-style: dashed;
    border-color: var(--om-border-strong);
    background: var(--om-surface);
    color: var(--om-text-muted);
    cursor: default;
    pointer-events: none;
  }
</style>
