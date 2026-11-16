<script lang="ts">
  import { ICONS, type IconName } from './icons';

  interface Props {
    name: IconName;
  }

  let { name }: Props = $props();
</script>

<!-- Decoration beside a word: the word (or the control's name) is what assistive technology reads. -->
<svg
  class="icon"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <!-- Constant markup from icons.ts, never user text. -->
  {@html ICONS[name]}
</svg>

<style>
  /* Sized from the type token of the text it sits beside, so it grows with the text. */
  .icon {
    flex: none;
    width: 1.25em;
    height: 1.25em;
  }
</style>
