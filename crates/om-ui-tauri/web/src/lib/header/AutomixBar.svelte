<script lang="ts">
  import Button from '../controls/Button.svelte';
  import { automixEnable, automixLength, automixPress, type AutomixBarState } from '../ipc';
  import type { Header } from './header.svelte';

  interface Props {
    bar: AutomixBarState;
    header: Header;
  }

  let { bar, header }: Props = $props();

  const progress = $derived(Math.round(Math.min(1, Math.max(0, bar.progress)) * 100));
  const nextText = $derived(bar.error === '' ? bar.next : bar.error);
</script>

<!-- The bar belongs to no deck: its progress is drawn in the text colour. -->
<div class="automix" role="group" aria-label="Automix">
  <span class="caption">AUTOMIX</span>
  <div class="switch" role="group" aria-label="Automix switch">
    <Button label="Automix off" pressed={!bar.enabled} on={!bar.enabled} onpress={() => void header.press(() => automixEnable(false))}>
      Off
    </Button>
    <Button label="Automix on" pressed={bar.enabled} on={bar.enabled} onpress={() => void header.press(() => automixEnable(true))}>
      On
    </Button>
  </div>
  {#if bar.enabled && !bar.running}
    <Button
      label="Transition length"
      title="Transition length"
      onpress={() => void header.press(() => automixLength(bar.nextLengthBeats))}
    >
      <span class="num">{bar.lengthLabel}</span>
    </Button>
  {/if}
  {#if bar.enabled && bar.running}
    <span class="state" role="status">{bar.state}</span>
  {/if}
  {#if bar.enabled || bar.error !== ''}
    <span class="next" title={nextText}>{nextText}</span>
  {/if}
  {#if bar.enabled && bar.remaining !== ''}
    <div class="progress" role="progressbar" aria-label="Transition" aria-valuemin="0" aria-valuemax="100" aria-valuenow={progress}>
      <div class="fill" style:width="{progress}%"></div>
    </div>
    <span class="remaining num">{bar.remaining}</span>
  {/if}
  {#if bar.canStart}
    <Button onpress={() => void header.press(() => automixPress('start'))}>Start</Button>
  {/if}
  {#if bar.canSkip}
    <Button onpress={() => void header.press(() => automixPress('skip'))}>Skip</Button>
  {/if}
  {#if bar.canCancel}
    <Button onpress={() => void header.press(() => automixPress('cancel'))}>Cancel</Button>
  {/if}
</div>

<style>
  .automix {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: var(--om-density-space-1);
    min-width: 0;
    margin: 0 var(--om-gap);
    font-size: var(--om-type-label);
    color: var(--om-text-primary);
  }

  .caption {
    flex: none;
    color: var(--om-text-muted);
    letter-spacing: 0.08em;
  }

  .switch {
    display: flex;
    flex: none;
  }

  .state {
    flex: none;
    min-width: 5ch;
    color: var(--om-text-secondary);
  }

  /* One line, cut with an ellipsis; an error is stated in words in the text colour (red is for
     the playhead and recording). */
  .next {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .progress {
    flex: none;
    width: 40px;
    height: var(--om-density-space-2);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-surface-raised);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--om-text-primary);
  }

  .remaining {
    flex: none;
    min-width: 12ch;
    color: var(--om-text-secondary);
    white-space: nowrap;
  }

  .num {
    font-family: var(--om-font-mono);
    font-size: var(--om-type-numeral);
    text-transform: none;
  }
</style>
