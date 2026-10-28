<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Toasts from '../components/Toasts.svelte'
  import { groups, isHot, track, type Extremes } from '../lib/monitor'
  import { live, startWindow } from '../lib/telemetry.svelte'
  import { errorText, toasts } from '../lib/toasts.svelte'

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

<div class="monitor">
  <table>
    <thead>
      <tr>
        <th scope="col">Sensor</th>
        <th scope="col">Value</th>
        <th scope="col">Min</th>
        <th scope="col">Max</th>
      </tr>
    </thead>
    {#each table as g (g.name)}
      <tbody>
        <tr class="group">
          <th scope="rowgroup" colspan="4"><span class="icon-cell"><Icon name={g.icon} />{g.name}</span></th>
        </tr>
        {#each g.rows as r (r.label)}
          {@const e = extremes[`${g.name}/${r.label}`]}
          <tr>
            <td>{r.label}</td>
            <td class="num" class:hot={r.value != null && isHot(r, r.value)}>{r.value == null ? '—' : r.show(r.value)}</td>
            <td class="num" class:secondary={!e || !isHot(r, e.min)} class:hot={e && isHot(r, e.min)}>{e ? r.show(e.min) : '—'}</td>
            <td class="num" class:secondary={!e || !isHot(r, e.max)} class:hot={e && isHot(r, e.max)}>{e ? r.show(e.max) : '—'}</td>
          </tr>
        {/each}
      </tbody>
    {/each}
  </table>

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

  tbody tr:not(.group):hover {
    background: var(--surface);
  }

  .group .icon-cell {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .hot {
    color: #ff99a4;
  }
</style>
