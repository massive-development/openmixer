<script lang="ts">
  import { letter } from '../a11y';

  interface Props {
    deck: number;
  }

  let { deck }: Props = $props();
</script>

<!-- The deck's letter on its hue: a hue is never shown without it. As GPUI's `parts::deck_tag`,
     a square the numeral size times the golden ratio. -->
<span class="tag" style:--hue="var(--om-accent-{deck})" aria-hidden="true">{letter(deck)}</span>

<style>
  .tag {
    display: grid;
    flex: none;
    place-items: center;
    width: calc(var(--om-type-numeral) * 1.618);
    height: calc(var(--om-type-numeral) * 1.618);
    border-radius: var(--om-control-radius);
    background: var(--hue);
    color: var(--om-on-accent);
    font-family: var(--om-font-mono);
    font-size: var(--om-type-numeral);
    font-weight: 700;
    line-height: 1;
  }
</style>
