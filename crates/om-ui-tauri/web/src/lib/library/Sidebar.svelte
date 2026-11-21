<script lang="ts">
  import type { Drag } from './dragger.svelte';
  import type { SidebarLine } from './ipc';

  interface Props {
    lines: SidebarLine[];
    drag: Drag;
    onselect: (index: number) => void;
  }

  let { lines, drag, onselect }: Props = $props();

  // A line that is a drop target: playlists take tracks, folders and the heading take playlists.
  function dropValue(line: SidebarLine): string | null {
    if (line.kind === 'playlist') return `playlist:${line.playlistId}`;
    if (line.kind === 'folder') return `folder:${line.playlistId}`;
    if (line.kind === 'playlists') return 'playlists';
    return null;
  }

  function press(event: PointerEvent, line: SidebarLine): void {
    if (line.kind !== 'playlist' && line.kind !== 'folder') return;
    drag.press(event, { kind: 'playlist', id: line.playlistId, label: line.label });
  }

  function click(line: SidebarLine): void {
    if (drag.justDragged()) return;
    onselect(line.index);
  }
</script>

<nav class="tree" aria-label="Library">
  {#each lines as line (line.index)}
    <button
      type="button"
      class="line"
      class:selected={line.selected}
      class:heading={!line.selectable}
      style:--depth={line.depth}
      aria-current={line.selected ? 'true' : undefined}
      data-drop={dropValue(line)}
      onpointerdown={(e) => press(e, line)}
      onclick={() => click(line)}
    >
      <span class="label">{line.label}</span>
    </button>
  {/each}
</nav>

<style>
  .tree {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: var(--om-density-space-1);
    overflow: hidden auto;
  }

  .line {
    position: relative;
    flex: none;
    height: var(--om-density-row-height);
    padding: 0 var(--om-inner-gap) 0 calc(var(--om-gap) + var(--depth) * 12px);
    border: 0;
    border-radius: var(--om-control-radius);
    background: none;
    color: var(--om-text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .line:hover {
    background: var(--om-surface-hover);
  }

  .line.heading {
    color: var(--om-text-muted);
    font-size: var(--om-type-label);
    text-transform: uppercase;
    cursor: default;
  }

  /* Selection is a fill and a bar in the focus colour, so it does not rest on tone alone. */
  .line.selected {
    background: var(--om-surface-hover);
    color: var(--om-text-primary);
  }

  .line.selected::before {
    content: '';
    position: absolute;
    left: 1px;
    top: 2px;
    bottom: 2px;
    width: 3px;
    border-radius: 1.5px;
    background: var(--om-focus-ring);
  }

  .label {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.line[data-drop-hot]) {
    background: var(--om-drop-target);
  }
</style>
