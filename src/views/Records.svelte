<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { onMount, tick } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import { api } from '../lib/api'
  import { duration } from '../lib/battery'
  import { rpm, temp, watts } from '../lib/format'
  import type { RecordEntry, Records } from '../lib/types'

  let records = $state<Records | null>(null)
  let failed = $state(false)

  const ROWS: { label: string; key: keyof Records; show: (v: number) => string }[] = [
    { label: 'Hottest CPU', key: 'cpuTempC', show: temp },
    { label: 'Hottest GPU', key: 'gpuTempC', show: temp },
    { label: 'Fastest fan', key: 'fanRpm', show: rpm },
    { label: 'Highest battery draw', key: 'batteryDrawW', show: watts },
    { label: 'Longest on battery', key: 'batteryStintMin', show: (v) => duration(Math.round(v)) },
  ]

  const date = (r: RecordEntry) =>
    new Date(r.atMs).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })

  onMount(() => {
    // Shown either way: an error in a window that never appears is no error at all.
    void api
      .getRecords()
      .then(
        (r) => (records = r),
        () => (failed = true),
      )
      .then(tick)
      .then(() => api.windowReady())
  })

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') void getCurrentWindow().close()
  }
</script>

<svelte:window {onkeydown} />

<div class="records">
  <header>
    <Icon name="star" size={20} />
    <h1>Records</h1>
  </header>

  {#if failed}
    <p class="secondary">Records couldn't be read.</p>
  {:else}
    <dl>
      {#each ROWS as row (row.key)}
        {@const r = records?.[row.key]}
        <div>
          <dt>
            {row.label}
            {#if r}<span class="secondary">{date(r)}</span>{/if}
          </dt>
          <dd class="num" class:secondary={!r}>{r ? row.show(r.value) : 'Not yet'}</dd>
        </div>
      {/each}
    </dl>
  {/if}

  <p class="note secondary">Temperatures and fans are recorded only while a window is open.</p>
</div>

<style>
  .records {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    padding: 16px;
    background: var(--panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-flyout);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  header > :global(.icon) {
    color: var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  dl {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
  }

  dl > div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    background: var(--surface);
    border-radius: 6px;
  }

  dt {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  dt .secondary {
    font-size: 12px;
  }

  dd {
    margin: 0;
    font-weight: 600;
  }

  dd.secondary {
    font-weight: 400;
  }

  .note {
    margin: auto 0 0;
    font-size: 12px;
  }
</style>
