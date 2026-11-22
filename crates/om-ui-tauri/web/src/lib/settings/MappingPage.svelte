<script lang="ts">
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { onDestroy, onMount } from 'svelte';
  import { mappingChange, mappingPage, usesLsb, type MappingChange, type MappingFilter, type MappingForm } from './devices';
  import { Pager } from './pager.svelte';
  import './shared.css';

  // The monitor and a finished learn follow the controller this often.
  const POLL_MS = 500;
  const MAP_FILTERS = [{ name: 'Controller map', extensions: ['json'] }];

  // The form is typed into here and sent on each key, so the shell's parse error follows the
  // typing; a page that arrives does not overwrite what is being typed.
  let draft: MappingForm | null = $state(null);
  // The binding the draft was made for: a poll replaces the draft only when another is selected
  // (a learn picked the new binding), never while the person types in this one.
  let shownFor: number | null = null;
  const pager = new Pager(mappingPage, (arrived, edit) => {
    const typing = draft !== null && arrived.form !== null;
    if (typing && edit) draft = { ...draft!, error: arrived.form!.error };
    else if (!typing || arrived.selected !== shownFor) draft = arrived.form;
    shownFor = arrived.selected;
  });
  const page = $derived(pager.page);
  let confirmReset = $state(false);
  let learnTarget = $state('');

  const change = (c: MappingChange): Promise<void> => pager.send(() => mappingChange(c));
  // A form edit keeps the typed text, and takes only the shell's verdict from the answer.
  function typed(next: Partial<MappingForm>): void {
    if (!draft) return;
    draft = { ...draft, ...next };
    const sent = draft;
    void pager.send(() => mappingChange({ field: 'setForm', value: sent }));
  }
  // Whole-form changes (select, add, undo...) replace the draft with the shell's form.
  const replace = (c: MappingChange): Promise<void> => {
    draft = null;
    return change(c);
  };
  const filterWith = (patch: Partial<MappingFilter>): void => {
    if (page) void change({ field: 'setFilter', value: { ...page.filter, ...patch } });
  };

  async function importMap(): Promise<void> {
    const file = await open({ multiple: false, directory: false, filters: MAP_FILTERS });
    if (typeof file === 'string') await replace({ field: 'import', value: file });
  }

  async function exportMap(): Promise<void> {
    const file = await save({ filters: MAP_FILTERS, defaultPath: 'controller-map.json' });
    if (file) await change({ field: 'export', value: file });
  }

  async function reset(): Promise<void> {
    confirmReset = false;
    await replace({ field: 'reset' });
  }

  onMount(() => pager.start(POLL_MS));
  onDestroy(() => pager.stop());
  $effect(() => {
    if (!learnTarget && page && page.targets.length > 0) learnTarget = page.targets[0].id;
  });
</script>

{#if !page}
  <p class="s-note">Loading the mapping editor…</p>
{:else if !page.available}
  <p class="s-note" role="status">The mapping editor needs MIDI, which is not available (see the terminal for the reason).</p>
{:else}
  <div class="s-stack" aria-busy={pager.busy}>
    <div class="s-row" role="toolbar" aria-label="Bindings">
      <button type="button" class="s-btn" onclick={() => void replace({ field: 'new' })}>Add</button>
      <button type="button" class="s-btn" disabled={page.selected === null} onclick={() => void replace({ field: 'duplicate' })}>Duplicate</button>
      <button type="button" class="s-btn" disabled={page.selected === null} onclick={() => void replace({ field: 'delete' })}>Delete</button>
      {#if page.learning}
        <button type="button" class="s-btn on" onclick={() => void change({ field: 'cancelLearn' })}>Cancel learn</button>
      {:else}
        <button type="button" class="s-btn" disabled={page.selected === null} onclick={() => void change({ field: 'learnSelected' })}>Learn selected</button>
      {/if}
      <button type="button" class="s-btn" onclick={() => void replace({ field: 'undo' })}>Undo</button>
      <button type="button" class="s-btn" onclick={() => void replace({ field: 'redo' })}>Redo</button>
      <button type="button" class="s-btn" onclick={() => void change({ field: 'save' })}>Save</button>
      <button type="button" class="s-btn" onclick={() => void importMap()}>Import…</button>
      <button type="button" class="s-btn" onclick={() => void exportMap()}>Export…</button>
      <button type="button" class="s-btn" onclick={() => (confirmReset = true)}>Reset…</button>
    </div>
    {#if confirmReset}
      <div class="s-row confirm" role="alertdialog" aria-label="Reset the map">
        <span class="s-note">Reset replaces every binding with an empty map. You can undo it; nothing is saved until you press Save.</span>
        <button type="button" class="s-btn" onclick={() => void reset()}>Reset the map</button>
        <button type="button" class="s-btn" onclick={() => (confirmReset = false)}>Keep it</button>
      </div>
    {/if}
    <p class="s-note path">
      Map: {page.mapFile}. Edits reach the running controller at once; Save writes the file.{page.dirty ? ' Unsaved changes.' : ''}
    </p>

    <div class="s-row" role="search" aria-label="Filter bindings">
      <input
        class="s-field"
        type="search"
        placeholder="Filter bindings"
        aria-label="Filter text"
        value={page.filter.text}
        oninput={(e) => filterWith({ text: e.currentTarget.value })}
      />
      <select class="s-field" aria-label="Group" value={page.filter.group ?? ''} onchange={(e) => filterWith({ group: e.currentTarget.value || null })}>
        <option value="">All groups</option>
        {#each page.groups as g (g)}<option value={g}>{g}</option>{/each}
      </select>
    </div>

    <div class="s-list table" role="listbox" aria-label="Bindings">
      {#each page.rows as row (row.index)}
        <button
          type="button"
          class="s-list-item"
          role="option"
          aria-selected={page.selected === row.index}
          onclick={() => void replace({ field: 'select', value: row.index })}
        >{row.conflicted ? '! ' : ''}{row.text}</button>
      {:else}
        <p class="s-note pad">No binding matches. Add one, or learn a control on the controller.</p>
      {/each}
    </div>

    {#if draft}
      <form class="form" onsubmit={(e) => { e.preventDefault(); void replace({ field: 'apply' }); }}>
        <label>Message type
          <select class="s-field" value={draft.kind} onchange={(e) => typed({ kind: e.currentTarget.value })}>
            {#each page.kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
          </select>
        </label>
        <label>Mode
          <select class="s-field" value={draft.mode} onchange={(e) => typed({ mode: e.currentTarget.value })}>
            {#each page.modes as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
          </select>
        </label>
        <label>Channel
          <input class="s-field num" title="MIDI channel (1-16, 0 = any)" value={draft.channel} oninput={(e) => typed({ channel: e.currentTarget.value })} />
        </label>
        <label>Number
          <input class="s-field num" title="controller or note number (0-127)" value={draft.number} oninput={(e) => typed({ number: e.currentTarget.value })} />
        </label>
        <label>Min
          <input class="s-field num" value={draft.min} oninput={(e) => typed({ min: e.currentTarget.value })} />
        </label>
        <label>Max
          <input class="s-field num" value={draft.max} oninput={(e) => typed({ max: e.currentTarget.value })} />
        </label>
        <label>Step
          <input class="s-field num" value={draft.step} oninput={(e) => typed({ step: e.currentTarget.value })} />
        </label>
        <label>Initial
          <input class="s-field num" value={draft.initial} oninput={(e) => typed({ initial: e.currentTarget.value })} />
        </label>
        {#if usesLsb(draft.mode)}
          <label>LSB controller
            <input class="s-field num" value={draft.lsb} oninput={(e) => typed({ lsb: e.currentTarget.value })} />
          </label>
        {/if}
        <label class="wide">Target
          <input
            class="s-field"
            list="map-targets"
            title="pick a control or type its id"
            value={draft.target}
            oninput={(e) => typed({ target: e.currentTarget.value })}
          />
          <datalist id="map-targets">
            {#each page.targets as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </datalist>
        </label>
        <label class="s-check"><input type="checkbox" checked={draft.invert} onchange={(e) => typed({ invert: e.currentTarget.checked })} /> Invert</label>
        {#if draft.error}<p class="s-error wide" role="status">Not valid yet: {draft.error}</p>{/if}
        <div class="s-row wide">
          <button type="submit" class="s-btn" disabled={draft.error !== null}>Apply</button>
        </div>
      </form>
    {:else}
      <p class="s-note">Select a binding to edit it, or press Add.</p>
    {/if}

    <div class="s-row">
      <select class="s-field" aria-label="Learn a new binding for" bind:value={learnTarget} disabled={page.learning}>
        {#each page.targets as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
      </select>
      <button type="button" class="s-btn" disabled={page.learning || !learnTarget} onclick={() => void replace({ field: 'learnNew', value: learnTarget })}>Learn new binding</button>
    </div>

    {#if page.conflicts.length > 0}
      <section class="s-stack" aria-label="Conflicts">
        <p class="s-note">Conflicts ({page.conflicts.length}): one control is bound twice. Keep one binding of each pair.</p>
        {#each page.conflicts as c (`${c.first}-${c.second}`)}
          <div class="s-row">
            <span class="conflict">{c.control}: bindings {c.first + 1} and {c.second + 1}</span>
            <button type="button" class="s-btn" onclick={() => void replace({ field: 'resolve', value: { first: c.first, second: c.second, keep: 'first' } })}>Keep first</button>
            <button type="button" class="s-btn" onclick={() => void replace({ field: 'resolve', value: { first: c.first, second: c.second, keep: 'second' } })}>Keep second</button>
          </div>
        {/each}
      </section>
    {/if}

    <section class="s-stack" aria-label="MIDI monitor">
      <h3 class="s-title">MIDI monitor (newest first)</h3>
      <div class="s-list monitor num" role="log" aria-label="MIDI events">
        {#each page.monitor as line, i (i)}<div>{line}</div>{:else}<div class="s-note">No MIDI event yet. Move a control on the controller.</div>{/each}
      </div>
    </section>

    {#if pager.notice}<p class="s-notice" class:error={pager.notice.error} role="status">{pager.notice.text}</p>{/if}
  </div>
{/if}

<style>
  h3 {
    font-weight: normal;
  }

  .path {
    overflow-wrap: anywhere;
  }

  .table {
    height: 150px;
  }

  .monitor {
    height: 90px;
    padding: var(--om-density-space-1) var(--om-inner-gap);
    font-size: var(--om-type-numeral);
    color: var(--om-text-secondary);
  }

  .pad {
    padding: var(--om-inner-gap);
  }

  .form {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--om-inner-gap) var(--om-gap);
    align-items: end;
  }

  .form label {
    display: flex;
    flex-direction: column;
    gap: var(--om-density-space-1);
    font-size: var(--om-type-label);
    color: var(--om-text-secondary);
  }

  .form label.s-check {
    flex-direction: row;
    align-items: center;
  }

  .wide {
    grid-column: 1 / -1;
  }

  .conflict {
    font-size: var(--om-type-body);
    color: var(--om-text-primary);
  }

  .confirm {
    padding: var(--om-inner-gap);
    border: 1px solid var(--om-border-strong);
    border-radius: var(--om-control-radius);
  }
</style>
