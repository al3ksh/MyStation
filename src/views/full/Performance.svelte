<script lang="ts">
  import Card from '../../components/Card.svelte'
  import LineChart from '../../components/LineChart.svelte'
  import ModeControls from '../../components/ModeControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { num, pct } from '../../lib/format'
  import { HISTORY_MINUTES } from '../../lib/history.svelte'
  import { driveNames, rate } from '../../lib/monitor'
  import { live } from '../../lib/telemetry.svelte'
  import type { HistorySample } from '../../lib/types'

  let { samples }: { samples: HistorySample[] } = $props()

  const app = $derived(live.app)
  const t = $derived(live.telemetry)
  const cpuLoad = $derived(samples.map((s) => ({ t: s.tsMs, v: s.cpuLoad })))
  const gpuLoad = $derived(samples.map((s) => ({ t: s.tsMs, v: s.gpuLoad })))
  const gpu = $derived(t?.gpu.state === 'active' ? t.gpu : null)
  const mem = $derived(t?.mem && t.mem.totalMb > 0 ? t.mem : null)
  const drives = $derived(t?.drives ?? [])
  const names = $derived(driveNames(drives))
  const gb = (mb: number) => num(mb / 1024, 1)
</script>

<div class="stack">
  <Card>
    <ModeControls battery={false} describe disabled={app ? !app.availability.cctk : false} />
  </Card>

  <div class="tiles">
    <div class="tile">
      <span class="name"><Icon name="cpu" size={14} />CPU</span>
      <span class="value num">{pct(t?.cpu.loadPct)}</span>
      {#if t?.cpu.coreLoads.length}<span class="detail secondary num">{t.cpu.coreLoads.length} threads</span>{/if}
    </div>
    <div class="tile">
      <span class="name"><Icon name="gpu" size={14} />GPU</span>
      {#if gpu}
        <span class="value num">{pct(gpu.loadPct)}</span>
        <span class="detail secondary num">
          {num(gpu.clockMhz, 0, ' MHz')}{#if gpu.vramUsedMb != null && gpu.vramTotalMb != null}
            · {gb(gpu.vramUsedMb)} / {gb(gpu.vramTotalMb)} GB{/if}
        </span>
      {:else}
        <span class="value secondary">{t?.gpu.state === 'asleep' ? 'Asleep' : '—'}</span>
      {/if}
    </div>
    <div class="tile">
      <span class="name"><Icon name="memory" size={14} />Memory</span>
      <span class="value num">{pct(mem ? (mem.usedMb / mem.totalMb) * 100 : null)}</span>
      {#if mem}<span class="detail secondary num">{gb(mem.usedMb)} / {gb(mem.totalMb)} GB</span>{/if}
    </div>
    {#each drives as d, i (d.index)}
      <div class="tile">
        <span class="name"><Icon name="drive" size={14} />{names[i]}</span>
        <span class="value num">{pct(d.activePct)}</span>
        {#if d.readBps != null && d.writeBps != null}
          <span class="detail secondary num">Read {rate(d.readBps)} · Write {rate(d.writeBps)}</span>
        {/if}
      </div>
    {/each}
  </div>

  <Card title="CPU load — last {HISTORY_MINUTES} minutes">
    {#snippet actions()}
      <span class="readout">
        {#if t?.cpu.limitPct != null}<span class="limit">Limited to {pct(t.cpu.limitPct)}</span>{/if}
        <span class="now num">{num(t?.cpu.clockMhz == null ? null : t.cpu.clockMhz / 1000, 2, ' GHz')}</span>
        <span class="now num">{pct(t?.cpu.loadPct)}</span>
      </span>
    {/snippet}
    <LineChart samples={cpuLoad} minutes={HISTORY_MINUTES} unit="%" min={0} max={100} />
  </Card>

  <Card title="GPU load — last {HISTORY_MINUTES} minutes">
    {#snippet actions()}
      <span class="now num">{gpu ? pct(gpu.loadPct) : t?.gpu.state === 'asleep' ? 'Asleep' : '—'}</span>
    {/snippet}
    <LineChart samples={gpuLoad} minutes={HISTORY_MINUTES} unit="%" min={0} max={100} />
  </Card>
</div>

<style>
  .stack {
    display: grid;
    gap: 12px;
    max-width: 880px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
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
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }

  .value {
    font-size: 18px;
    font-weight: 400;
  }

  .detail {
    font-size: 12px;
  }

  .now {
    font-weight: 600;
  }

  .readout {
    display: flex;
    gap: 12px;
  }

  .limit {
    color: var(--accent);
    font-weight: 600;
  }
</style>
