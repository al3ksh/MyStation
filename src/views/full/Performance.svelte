<script lang="ts">
  import Card from '../../components/Card.svelte'
  import LineChart from '../../components/LineChart.svelte'
  import ModeControls from '../../components/ModeControls.svelte'
  import { num, pct, temp } from '../../lib/format'
  import { HISTORY_MINUTES } from '../../lib/history.svelte'
  import { live } from '../../lib/telemetry.svelte'
  import type { HistorySample } from '../../lib/types'

  let { samples }: { samples: HistorySample[] } = $props()

  const app = $derived(live.app)
  const t = $derived(live.telemetry)
  const cpuTemp = $derived(samples.map((s) => ({ t: s.tsMs, v: s.cpuTemp })))
  const cpuLoad = $derived(samples.map((s) => ({ t: s.tsMs, v: s.cpuLoad })))
</script>

<div class="stack">
  <Card>
    <ModeControls battery={false} describe disabled={app ? !app.availability.cctk : false} />
  </Card>

  <Card title="CPU temperature — last {HISTORY_MINUTES} minutes">
    {#snippet actions()}<span class="now num">{temp(t?.cpu.tempC)}</span>{/snippet}
    <LineChart samples={cpuTemp} minutes={HISTORY_MINUTES} unit="°" />
  </Card>

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
</div>

<style>
  .stack {
    display: grid;
    gap: 12px;
    max-width: 880px;
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
