<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Toasts from '../components/Toasts.svelte'
  import { groups, isHot, track, type Extremes } from '../lib/monitor'
  import { live, startWindow } from '../lib/telemetry.svelte'
  import { errorText, toasts } from '../lib/toasts.svelte'

  /** Groups folded away, kept for the next time the window opens. */
  const FOLDED_KEY = 'monitor.folded'
  /** The load of every thread is long, so it starts folded. */
  const FOLDED_AT_FIRST = ['CPU threads']

  function loadFolded(): string[] {
    try {
      const v = JSON.parse(localStorage.getItem(FOLDED_KEY) ?? 'null')
      return Array.isArray(v) ? v : FOLDED_AT_FIRST
    } catch {
      return FOLDED_AT_FIRST
    }
  }

  let folded = $state(loadFolded())

  function toggle(name: string) {
    folded = folded.includes(name) ? folded.filter((n) => n !== name) : [...folded, name]
    localStorage.setItem(FOLDED_KEY, JSON.stringify(folded))
  }

  let extremes = $state<Extremes>({})
  let now = $state(Date.now())

  const table = $derived(groups(live.telemetry, live.app, now))

  // Each tick widens min and max; nothing else should.
  $effect(() => {
    const t = live.telemetry
    if (!t)
      return
    untrack(() => {
      now = Date.now()
      extremes = track(extremes, groups(t, live.app, now))
    })
  })

  onMount(() => void startWindow((e) => toasts.push(errorText(e))))
  onDestroy(() => live.stop())
</script>

{#snippet cols()}
  <colgroup><col class="label" /><col /><col /><col /></colgroup>
{/snippet}

<div class="monitor">
  <!-- The headers stay put above the rows: the window is see-through, so rows scrolling under a
       sticky header would show through it. -->
  <div class="head">
    <table>
      {@render cols()}
      <thead>
        <tr>
          <th scope="col">Sensor</th>
          <th scope="col">Value</th>
          <th scope="col">Min</th>
          <th scope="col">Max</th>
        </tr>
      </thead>
    </table>
  </div>
  <div class="rows">
    <table>
      {@render cols()}
      {#each table as g (g.name)}
        {@const open = !folded.includes(g.name)}
        <tbody>
          <tr class="group">
            <th scope="rowgroup" colspan="4">
              <button type="button" class="icon-cell" aria-expanded={open} onclick={() => toggle(g.name)}>
                <span class="chevron" class:open><Icon name="down" /></span><Icon name={g.icon} />{g.name}
              </button>
            </th>
          </tr>
          {#if open}
            {#each g.rows as r (r.label)}
              {@const e = extremes[`${g.name}/${r.label}`]}
              <tr>
                <td>{r.label}</td>
                <td class="num" class:hot={r.value != null && isHot(r, r.value)}>{r.value == null ? '—' : r.show(r.value)}</td>
                <td class="num" class:secondary={!e || !isHot(r, e.min)} class:hot={e && isHot(r, e.min)}>{e ? r.show(e.min) : '—'}</td>
                <td class="num" class:secondary={!e || !isHot(r, e.max)} class:hot={e && isHot(r, e.max)}>{e ? r.show(e.max) : '—'}</td>
              </tr>
            {/each}
          {/if}
        </tbody>
      {/each}
    </table>
  </div>

  <Toasts floating />
</div>

<style>
  .monitor {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--layer);
  }

  .head,
  .rows {
    /* Both reserve the scrollbar's width, so their columns line up. */
    scrollbar-gutter: stable;
  }

  .head {
    overflow: hidden;
    border-bottom: 1px solid var(--border);
  }

  .rows {
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-bottom: 12px;
  }

  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
    font-size: 13px;
  }

  col.label {
    width: 46%;
  }

  thead th {
    padding: 8px 12px;
    font-weight: 600;
  }

  th,
  td {
    padding: 4px 12px;
    text-align: right;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  th:first-child,
  td:first-child {
    text-align: left;
  }

  td:first-child {
    padding-left: 52px;
  }

  .group th {
    padding: 6px 6px 2px;
  }

  tbody tr:not(.group):hover {
    background: var(--surface);
  }

  .group button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 6px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  .group button:hover {
    background: var(--surface);
  }

  .group button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .chevron {
    display: inline-flex;
    font-size: 10px;
    color: var(--text-2);
    transform: rotate(-90deg);
    transition: transform 0.15s var(--ease);
  }

  .chevron.open {
    transform: none;
  }

  .hot {
    color: #ff99a4;
  }
</style>
