<script lang="ts">
  import { onMount } from 'svelte';
  import Button from '../controls/Button.svelte';
  import { LibraryBrowser } from './browser.svelte';
  import type { Payload, Target } from './drag';
  import { Drag } from './dragger.svelte';
  import {
    libraryActivate,
    libraryAnalyse,
    libraryClearSearch,
    libraryCopy,
    libraryDeletePlaylist,
    libraryDropOnPlaylist,
    libraryDropOnRow,
    libraryDropPlaylist,
    libraryKey,
    libraryLoad,
    libraryName,
    libraryRemoveFromPlaylist,
    librarySearch,
    librarySelectRow,
    librarySelectSidebar,
    librarySort,
    libraryStems,
    type NameAction,
  } from './ipc';
  import { clock, prepSummary } from './rows';
  import Sidebar from './Sidebar.svelte';
  import TrackTable from './TrackTable.svelte';

  const browser = new LibraryBrowser();
  const view = $derived(browser.view);

  // A drop on a deck, playlist or row, or a playlist moved into a folder.
  const drag = new Drag((payload: Payload, target: Target) => {
    if (payload.kind === 'track') {
      if (target.kind === 'deck') void browser.run((w) => libraryLoad(target.deck, payload.trackId, w));
      else if (target.kind === 'playlist')
        void browser.run((w) => libraryDropOnPlaylist(target.id, payload.trackId, w));
      else if (target.kind === 'row') void browser.run((w) => libraryDropOnRow(payload.row, target.row, w));
    } else if (target.kind === 'folder') {
      void browser.run((w) => libraryDropPlaylist(payload.id, target.id, w));
    }
  });

  let windowWidth = $state(window.innerWidth);
  let search: HTMLInputElement | undefined = $state();
  let root: HTMLElement | undefined = $state();
  let follow: number | null = $state(null);
  // The box's own text while it is focused: it is what the user types, whatever the shell last said.
  let typed = $state('');
  let boxFocused = $state(false);

  const naming = $derived(view?.naming ?? null);
  const boxText = $derived(boxFocused ? typed : (naming?.text ?? view?.search ?? ''));

  onMount(() => browser.start());

  // Naming a playlist takes the box: it shows the prompt and the text so far, and takes the keyboard.
  $effect(() => {
    if (naming && search && document.activeElement !== search) {
      typed = naming.text;
      search.focus();
    }
  });

  function type(text: string): void {
    typed = text;
    if (naming) void browser.run((w) => libraryName('text', text, w));
    else void browser.run((w) => librarySearch(text, false, w));
  }

  function boxKey(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      if (naming) void browser.run((w) => libraryName('submit', null, w));
      else void browser.run((w) => librarySearch(typed, true, w));
    } else if (event.key === 'Escape') {
      event.preventDefault();
      typed = '';
      if (naming) void browser.run((w) => libraryName('cancel', null, w));
      else void browser.run((w) => libraryClearSearch(w));
      root?.focus();
    }
  }

  async function copy(): Promise<void> {
    const text = await libraryCopy().catch(() => '');
    if (text) await navigator.clipboard?.writeText(text).catch(() => undefined);
  }

  // Keys for the table while no text box has the keyboard.
  async function key(event: KeyboardEvent): Promise<void> {
    if (event.target instanceof HTMLInputElement) return;
    if ((event.ctrlKey || event.metaKey) && event.key === 'c') {
      event.preventDefault();
      await copy();
      return;
    }
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const name = ({ ArrowUp: 'up', ArrowDown: 'down', Enter: 'enter', p: 'p', '/': '/' } as Record<string, string>)[event.key];
    if (!name) return;
    event.preventDefault();
    const outcome = await browser.runWith((w) => libraryKey(name, w));
    if (outcome === 'search') search?.focus();
    else if (outcome === 'moved') follow = browser.view?.selected[0] ?? null;
  }

  const name = (action: NameAction): void => void browser.run((w) => libraryName(action, null, w));
  const status = $derived(view?.status ?? '');
  const analysis = $derived(view?.analysis ?? null);
  const queue = $derived(analysis ? prepSummary(analysis.queued, analysis.running, analysis.failed) : '');
  const selectedTrack = $derived(analysis?.track ?? null);
</script>

<svelte:window bind:innerWidth={windowWidth} />

<!-- The card takes the keyboard for the table (arrows, Enter, p, /, Ctrl+C), as the window's library did. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<section class="library" bind:this={root} tabindex="0" aria-label="Library" onkeydown={key}>
  {#if browser.problem && !view}
    <p class="problem" role="status">Library could not open. Open Settings &gt; Library. ({browser.problem})</p>
  {:else if view}
    <Sidebar lines={view.sidebar} {drag} onselect={(i) => void browser.run((w) => librarySelectSidebar(i, w))} />
    <div class="main">
      <div class="toolbar">
        <input
          class="search"
          type="search"
          bind:this={search}
          value={boxText}
          placeholder={naming?.prompt ?? view.placeholder}
          aria-label={naming?.prompt ?? 'Search the library'}
          onfocus={() => {
            typed = boxText;
            boxFocused = true;
          }}
          onblur={() => (boxFocused = false)}
          oninput={(e) => type(e.currentTarget.value)}
          onkeydown={boxKey}
        />
        <Button onpress={() => void copy()}>Copy</Button>
        <Button onpress={() => void browser.run((w) => libraryAnalyse(w))}>Analyse</Button>
        {#if view.stemsAvailable}
          <Button onpress={() => void browser.run((w) => libraryStems(false, w))}>Split stems</Button>
          {#if view.stemsInFlight > 0}
            <Button onpress={() => void browser.run((w) => libraryStems(true, w))}>Cancel stems</Button>
          {/if}
        {/if}
        <span class="rule" aria-hidden="true"></span>
        <Button onpress={() => name('new-playlist')}>New</Button>
        <Button onpress={() => name('rename')}>Rename</Button>
        <Button onpress={() => name('new-folder')}>New folder</Button>
        <span class="spread"></span>
        <span class="rule" aria-hidden="true"></span>
        <Button onpress={() => void browser.run((w) => libraryDeletePlaylist(w))}>Delete playlist</Button>
        <Button onpress={() => void browser.run((w) => libraryRemoveFromPlaylist(w))}>Remove from playlist</Button>
      </div>
      <div class="status" class:error={view.statusIsError}>
        <span class="line" role="status">{status}</span>
        {#if queue}
          <span class="queue" title="The analysis queue">
            {queue}
            {#if analysis && analysis.running > 0}
              <span class="bar" role="progressbar" aria-label="Analysis progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(analysis.progress * 100)}>
                <span class="fill" style:width="{Math.round(analysis.progress * 100)}%"></span>
              </span>
            {/if}
          </span>
        {/if}
        {#if selectedTrack}
          <span class="track" aria-label="Selected track analysis">
            <span class="num">{selectedTrack.bpm ? `${selectedTrack.bpm.toFixed(1)} BPM` : '-- BPM'}</span>
            <span class="num">{selectedTrack.camelot || selectedTrack.key || '--'}</span>
            <span class="num">{clock(selectedTrack.durationSec)}</span>
            <span class="state" class:pending={!selectedTrack.analysed}>
              {selectedTrack.prep === 'running'
                ? 'Analysing'
                : selectedTrack.prep === 'queued'
                  ? 'Queued'
                  : selectedTrack.prep === 'failed'
                    ? 'Analysis failed'
                    : selectedTrack.analysed
                      ? 'Analysed'
                      : 'Not analysed'}
            </span>
          </span>
        {/if}
      </div>
      <TrackTable
        {view}
        {drag}
        {windowWidth}
        {follow}
        onwindow={(first, count) => browser.show({ first, count })}
        onsort={(c) => void browser.run((w) => librarySort(c, w))}
        onselect={(row, extend) => void browser.run((w) => librarySelectRow(row, extend, w))}
        onactivate={(row) => void browser.run((w) => libraryActivate(row, w))}
      />
    </div>
  {:else}
    <p class="problem" role="status">Opening the library…</p>
  {/if}
</section>

{#if drag.ghost}
  <div class="ghost" style:left="{drag.ghost.x + 12}px" style:top="{drag.ghost.y + 12}px">{drag.ghost.label}</div>
{/if}

<style>
  .library {
    display: grid;
    grid-template-columns: var(--om-library-sidebar-width) minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    background: var(--om-surface);
    color: var(--om-text-primary);
    outline-offset: -2px;
  }

  .main {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    padding: var(--om-density-space-1) var(--om-density-space-2) 0;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--om-density-space-1);
    min-height: var(--om-density-control-height);
  }

  .search {
    flex: 4 1 var(--om-library-search-min-width);
    min-width: 0;
    height: var(--om-density-control-height);
    box-sizing: border-box;
    padding: 0 var(--om-inner-gap);
    border: var(--om-border-width) solid var(--om-border);
    border-radius: var(--om-control-radius);
    background: var(--om-well);
    color: var(--om-text-primary);
    font: inherit;
    -webkit-user-select: text;
    user-select: text;
  }

  .search::placeholder {
    color: var(--om-text-muted);
  }

  .rule {
    flex: none;
    width: var(--om-border-width);
    height: 60%;
    margin: 0 var(--om-density-space-1);
    background: var(--om-border);
  }

  .spread {
    flex: 1 1 0;
  }

  .status {
    display: flex;
    align-items: center;
    gap: var(--om-gap);
    min-height: var(--om-density-control-height);
    overflow: hidden;
    color: var(--om-text-secondary);
    font-size: var(--om-type-label);
    white-space: nowrap;
  }

  /* An error states its reason in words at full text strength; never red. */
  .status.error {
    color: var(--om-text-primary);
  }

  .line {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .queue,
  .track {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--om-inner-gap);
    color: var(--om-text-secondary);
  }

  .bar {
    display: inline-block;
    width: 64px;
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--om-well);
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--om-master);
  }

  .state.pending {
    color: var(--om-text-muted);
  }

  .problem {
    grid-column: 1 / -1;
    margin: 0;
    padding: var(--om-gap);
    color: var(--om-text-secondary);
  }

  .ghost {
    position: fixed;
    z-index: 10;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    border: var(--om-border-width) solid var(--om-border-strong);
    border-radius: var(--om-control-radius);
    background: var(--om-popup);
    color: var(--om-text-primary);
    font-size: var(--om-type-label);
    pointer-events: none;
  }
</style>
