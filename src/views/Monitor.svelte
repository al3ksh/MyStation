<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import Toasts from '../components/Toasts.svelte'
  import { api } from '../lib/api'
  import { groups, recordShow, track, type Extremes } from '../lib/monitor'
  import { live, startWindow } from '../lib/telemetry.svelte'
  import { errorText, toasts } from '../lib/toasts.svelte'
  import type { RecordEntry, Records } from '../lib/types'

  /** Records move only while something runs hot; a minute is soon enough to see it. */
  const RECORDS_REFRESH_MS = 60_000

  let records = $state<Records | null>(null)
  let extremes = $state<Extremes>({})
  let now = $state(Date.now())
  let timer: ReturnType<typeof setInterval> | undefined

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

  const loadRecords = () =>
    api.getRecords().then(
      (r) => (records = r),
      (e) => toasts.push(errorText(e)),
    )

  onMount(() => {
    const loaded = loadRecords()
    timer = setInterval(loadRecords, RECORDS_REFRESH_MS)
    void startWindow((e) => toasts.push(errorText(e)), () => loaded)
  })
  onDestroy(() => {
    clearInterval(timer)
    live.stop()
  })

  const when = (r: RecordEntry) =>
    new Date(r.atMs).toLocaleString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    })
</script>

{#snippet record(key: keyof Records | undefined)}
  {@const r = key ? records?.[key] : null}
  {#if key && r}
    <td class="num" title={when(r)}>{recordShow[key](r.value)}</td>
  {:else if key}
    <td class="num secondary">—</td>
  {:else}
    <td></td>
  {/if}
{/snippet}

<div class="monitor">
  <table>
    <thead>
      <tr>
        <th scope="col">Sensor</th>
        <th scope="col">Value</th>
        <th scope="col">Min</th>
        <th scope="col">Max</th>
        <th scope="col">Record</th>
      </tr>
    </thead>
    {#each table as g (g.name)}
      <tbody>
        <tr class="group">
          <th scope="rowgroup" colspan="4">{g.name}</th>
          {@render record(g.record)}
        </tr>
        {#each g.rows as r (r.label)}
          {@const e = extremes[`${g.name}/${r.label}`]}
          <tr>
            <td>{r.label}</td>
            <td class="num">{r.value == null ? '—' : r.show(r.value)}</td>
            <td class="num secondary">{e ? r.show(e.min) : '—'}</td>
            <td class="num secondary">{e ? r.show(e.max) : '—'}</td>
            {@render record(r.record)}
          </tr>
        {/each}
      </tbody>
    {/each}
  </table>

  <p class="note secondary">
    Min and max are since this window opened. Records are all-time; temperatures and fans are recorded only while a
    window is open.
  </p>

  <Toasts floating />
</div>

<style>
  .monitor {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0 0 12px;
    background: var(--layer);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 8px 12px;
    font-weight: 600;
    background: var(--layer);
    border-bottom: 1px solid var(--border);
  }

  th,
  td {
    padding: 4px 12px;
    text-align: right;
    white-space: nowrap;
  }

  th:first-child,
  td:first-child {
    text-align: left;
  }

  td:first-child {
    padding-left: 28px;
  }

  .group th {
    padding-top: 10px;
    font-weight: 600;
    text-align: left;
  }

  .group td {
    padding-top: 10px;
  }

  tbody tr:not(.group):hover {
    background: var(--surface);
  }

  .note {
    margin: 12px 12px 0;
    font-size: 12px;
  }
</style>
