<script lang="ts">
  import Card from '../../components/Card.svelte'
  import Icon from '../../components/Icon.svelte'
  import LineChart from '../../components/LineChart.svelte'
  import { api } from '../../lib/api'
  import { num, rpm, temp } from '../../lib/format'
  import { HISTORY_MINUTES } from '../../lib/history.svelte'
  import { live } from '../../lib/telemetry.svelte'
  import { errorText, toasts } from '../../lib/toasts.svelte'
  import type { HistorySample } from '../../lib/types'

  let { samples }: { samples: HistorySample[] } = $props()

  const t = $derived(live.telemetry)
  const gpuTemp = $derived(samples.map((s) => ({ t: s.tsMs, v: s.gpuTemp })))

  const gpuNow = $derived.by(() => {
    const g = t?.gpu
    if (!g) return '—'
    if (g.state === 'asleep') return 'Asleep'
    if (g.state === 'unavailable') return 'Unavailable'
    return temp(g.tempC)
  })
</script>

<div class="stack">
  <!-- Every reading in one table, in a window of its own that can stay open beside others. -->
  <button type="button" class="btn" onclick={() => api.openMonitor().catch((e) => toasts.push(errorText(e)))}>
    <Icon name="openWindow" /> Open sensor monitor
  </button>

  <div class="tiles">
    <div class="tile"><span class="name">Memory module</span><span class="value num">{temp(t?.dimmC)}</span></div>
    <div class="tile"><span class="name">Chassis</span><span class="value num">{temp(t?.skinC)}</span></div>
    <div class="tile">
      <span class="name">Fans</span>
      {#if t && t.fans.length}
        {#each t.fans as f (f.name)}
          <span class="fan num"><span class="secondary">{f.name}</span> {rpm(f.rpm)}</span>
        {/each}
      {:else}
        <span class="value">—</span>
      {/if}
    </div>
    <div class="tile">
      <span class="name">RAM</span>
      <span class="value num">
        {t?.mem ? `${num(t.mem.usedMb / 1024, 1)} / ${num(t.mem.totalMb / 1024, 1)} GB` : '—'}
      </span>
    </div>
  </div>

  <Card title="GPU temperature — last {HISTORY_MINUTES} minutes">
    {#snippet actions()}<span class="now num">{gpuNow}</span>{/snippet}
    <LineChart samples={gpuTemp} minutes={HISTORY_MINUTES} unit="°" />
  </Card>
</div>

<style>
  .stack {
    display: grid;
    gap: 12px;
    max-width: 880px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    justify-self: start;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
    cursor: default;
  }

  .btn:hover {
    background: var(--surface-raised);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }

  .tile {
    display: grid;
    align-content: start;
    gap: 2px;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .name {
    font-size: 12px;
    color: var(--text-2);
  }

  .value {
    font-size: 18px;
    font-weight: 400;
  }

  .fan {
    font-size: 12px;
  }

  .now {
    font-weight: 500;
  }
</style>
