<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    on?: boolean;
    /** A deck or cue colour: the on-state is tinted in it (border and fill) instead of neutral. */
    tint?: string | null;
    /** The neutral on-state's border colour, when it is not the strong border. */
    onBorder?: string | null;
    tall?: boolean;
    title?: string;
    onpress: (event: MouseEvent) => void;
    children: Snippet;
  }

  let { on = false, tint = null, onBorder = null, tall = false, title, onpress, children }: Props = $props();
</script>

<button
  type="button"
  class="btn"
  class:on
  class:tinted={on && tint !== null}
  class:tall
  style:--tint={tint}
  style:--on-border={onBorder}
  {title}
  onclick={onpress}
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
  }

  .tall {
    min-height: var(--om-min-target);
    font-size: var(--om-type-body);
  }

  .btn:hover {
    background: var(--om-surface-hover);
  }

  .btn:active {
    background: var(--om-border);
  }

  .on {
    color: var(--om-text-primary);
    border-color: var(--on-border, var(--om-border-strong));
    background: var(--om-surface-hover);
  }

  /* The tint is two layers at the theme's deck alphas, so any theme colour works without mixing. */
  .tinted {
    color: var(--tint);
    border-color: transparent;
    background: var(--om-surface-raised);
  }

  .tinted::before,
  .tinted::after {
    content: '';
    position: absolute;
    inset: calc(-1 * var(--om-border-width));
    border-radius: inherit;
    pointer-events: none;
  }

  .tinted::before {
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
</style>
