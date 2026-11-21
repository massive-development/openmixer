<script lang="ts">
  import { onMount } from 'svelte';
  import type { Drag } from './dragger.svelte';
  import type { LibraryView, Row } from './ipc';
  import { ariaSort, columnWidths, deckLetter, filledStars, rowWindow, shownColumns, sortGlyph } from './rows';

  interface Props {
    view: LibraryView;
    drag: Drag;
    /** The window's width, which decides the columns shown. */
    windowWidth: number;
    onwindow: (first: number, count: number) => void;
    onsort: (column: number) => void;
    onselect: (row: number, extend: boolean) => void;
    onactivate: (row: number) => void;
    /** The row whose track keys moved to: scrolled into view. */
    follow: number | null;
  }

  let { view, drag, windowWidth, onwindow, onsort, onselect, onactivate, follow }: Props = $props();

  let scroller: HTMLElement | undefined = $state();
  let listWidth = $state(800);
  let viewport = $state(200);
  let scrollTop = $state(0);
  // The row height is a theme token; read once the page has it.
  let rowHeight = $state(24);

  const shown = $derived(shownColumns(view.columns, windowWidth));
  const widths = $derived(columnWidths(shown, listWidth));
  const index = $derived(shown.map((c) => view.columns.findIndex((v) => v.id === c.id)));
  const total = $derived(view.rowCount * rowHeight);

  $effect(() => {
    const w = rowWindow(scrollTop, viewport, rowHeight, view.rowCount);
    onwindow(w.first, w.count);
  });

  // Keys moved the selection: bring its row into view.
  $effect(() => {
    if (follow === null || !scroller) return;
    const top = follow * rowHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + rowHeight > scroller.scrollTop + scroller.clientHeight)
      scroller.scrollTop = top + rowHeight - scroller.clientHeight;
  });

  onMount(() => {
    if (!scroller) return;
    const token = parseFloat(getComputedStyle(scroller).getPropertyValue('--om-density-row-height'));
    if (token > 0) rowHeight = token;
    const watch = new ResizeObserver(() => {
      if (!scroller) return;
      viewport = scroller.clientHeight;
      listWidth = scroller.clientWidth;
    });
    watch.observe(scroller);
    return () => watch.disconnect();
  });

  function press(event: PointerEvent, row: Row): void {
    drag.press(event, {
      kind: 'track',
      trackId: row.trackId,
      row: row.row,
      label: row.cells[0] ?? '',
    });
  }

  function click(event: MouseEvent, row: Row): void {
    if (drag.justDragged()) return;
    onselect(row.row, event.shiftKey);
  }

  const template = $derived(widths.map((w) => `${w}px`).join(' '));
</script>

<div class="table" role="grid" aria-label="Tracks" aria-rowcount={view.rowCount}>
  <div class="head" role="row" style:grid-template-columns={template} style:width="{listWidth}px">
    {#each shown as column, at (column.id)}
      {#if column.sortable}
        <button
          type="button"
          class="th"
          class:numeric={column.numeric}
          role="columnheader"
          aria-sort={ariaSort(view.sort, column.id)}
          onclick={() => onsort(column.id)}
          style:width="{widths[at]}px"
        >
          {column.title}<span class="glyph" aria-hidden="true">{sortGlyph(view.sort, column.id)}</span>
        </button>
      {:else}
        <span class="th plain" role="columnheader" style:width="{widths[at]}px">{column.title}</span>
      {/if}
    {/each}
  </div>
  <div class="body" bind:this={scroller} onscroll={() => scroller && (scrollTop = scroller.scrollTop)}>
    {#if view.rowCount === 0}
      <p class="empty">No tracks match</p>
    {/if}
    <div class="spacer" style:height="{total}px">
      {#each view.rows as row (row.row)}
        <!-- Keys (arrows, Enter) are the table's own, handled on the library card. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:selected={row.selected}
          class:compatible={row.compatible}
          class:strong={row.mix === 'strong'}
          class:broken={row.error !== null}
          class:odd={row.row % 2 === 1}
          style:top="{row.row * rowHeight}px"
          style:height="{rowHeight}px"
          style:grid-template-columns={template}
          style:--deck={row.onDeck === null ? null : `var(--om-accent-${row.onDeck})`}
          role="row"
          aria-selected={row.selected}
          aria-rowindex={row.row + 1}
          data-drop="row:{row.row}"
          data-track={row.trackId}
          tabindex="-1"
          onpointerdown={(e) => press(e, row)}
          onclick={(e) => click(e, row)}
          ondblclick={() => onactivate(row.row)}
        >
          {#each shown as column, at (column.id)}
            {@const cell = row.cells[index[at]] ?? ''}
            <div class="td" class:num={column.numeric} class:numeric={column.numeric} role="gridcell">
              {#if column.title === 'Rating'}
                <span class="stars" role="img" aria-label="{filledStars(cell)} of 5 stars">
                  {#each Array.from(cell) as star, s (s)}<span class:off={star !== '*'}>★</span>{/each}
                </span>
              {:else if at === 0}
                {#if row.onDeck !== null}<span class="mark" style:color="var(--om-accent-{row.onDeck})">{deckLetter(row.onDeck)}</span>{/if}
                <span class="text">{cell}</span>
                {#if row.error}<span class="reason">{row.error}</span>{/if}
                {#if row.preparing !== null}<span class="prep" title="Analysing">…</span>{/if}
              {:else}
                <span class="text">{cell}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .table {
    display: grid;
    grid-template-rows: var(--om-density-control-height) minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    font-size: var(--om-type-body);
  }

  .head {
    display: grid;
    align-items: center;
    border-bottom: var(--om-border-width) solid var(--om-border);
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
  }

  .th {
    min-width: 0;
    padding: 0 var(--om-inner-gap);
    overflow: hidden;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .th.plain {
    cursor: default;
  }

  .th:hover:not(.plain) {
    color: var(--om-text-primary);
  }

  .numeric {
    text-align: right;
    justify-content: flex-end;
  }

  .glyph {
    display: inline-block;
    min-width: 1em;
    color: var(--om-text-secondary);
  }

  .body {
    position: relative;
    overflow: hidden auto;
  }

  .spacer {
    position: relative;
  }

  .empty {
    position: absolute;
    margin: 0;
    padding: var(--om-inner-gap) var(--om-gap);
    color: var(--om-text-muted);
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    align-items: center;
    box-sizing: border-box;
    border: 1px solid transparent;
    color: var(--om-text-primary);
    cursor: default;
  }

  .row.odd {
    background: color-mix(in srgb, var(--om-text-primary) calc(var(--om-library-stripe-alpha) * 100%), transparent);
  }

  /* Key fits the reference deck; stronger when the tempo fits too. */
  .row.compatible {
    background: color-mix(in srgb, var(--om-master) calc(var(--om-library-compat-alpha) * 50%), transparent);
  }

  .row.compatible.strong {
    background: color-mix(in srgb, var(--om-master) calc(var(--om-library-compat-alpha) * 100%), transparent);
  }

  /* On a deck: that deck's edge. */
  .row[style*='--deck'] {
    border-color: color-mix(in srgb, var(--deck) 40%, transparent);
  }

  /* Selection is the fill (the slate hue, deck B's), with its own edge unless the row has one. */
  .row.selected {
    background: color-mix(in srgb, var(--om-accent-1) 18%, transparent);
    border-color: color-mix(in srgb, var(--om-accent-1) 55%, transparent);
  }

  .row.selected[style*='--deck'] {
    border-color: color-mix(in srgb, var(--deck) 40%, transparent);
  }

  /* A track that would not load: dashed edge and dimmed, never red. */
  .row.broken {
    border: 1px dashed var(--om-border-strong);
    color: var(--om-text-muted);
  }

  .td {
    display: flex;
    align-items: center;
    gap: var(--om-density-space-2);
    min-width: 0;
    padding: 0 var(--om-inner-gap);
    overflow: hidden;
    white-space: nowrap;
  }

  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mark {
    flex: none;
    font-family: var(--om-font-display);
    font-weight: 700;
  }

  .reason {
    flex: none;
    color: var(--om-text-secondary);
    font-size: var(--om-type-label);
  }

  .prep {
    flex: none;
    color: var(--om-text-muted);
  }

  .stars {
    display: inline-flex;
    color: var(--om-text-secondary);
  }

  .stars .off {
    opacity: 0.35;
  }

  /* A drag over a playlist's row would reorder it. */
  :global(.row[data-drop-hot]) {
    background: var(--om-drop-target);
  }
</style>
