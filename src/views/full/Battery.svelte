<script lang="ts">
  import { onMount } from 'svelte'
  import Card from '../../components/Card.svelte'
  import LineChart from '../../components/LineChart.svelte'
  import ModeControls from '../../components/ModeControls.svelte'
  import { api } from '../../lib/api'
  import { num, pct, watts, wh } from '../../lib/format'
  import { HISTORY_MINUTES } from '../../lib/history.svelte'
  import { duration, timeLeftMin } from '../../lib/battery'
  import { chargeCfgLabel, PROFILE_LABEL } from '../../lib/labels'
  import { live } from '../../lib/telemetry.svelte'
  import { errorText, toasts } from '../../lib/toasts.svelte'
  import type { BatteryProfile, HealthEntry, HistorySample } from '../../lib/types'

  let { samples }: { samples: HistorySample[] } = $props()

  let health = $state<HealthEntry[]>([])
  onMount(() => {
    api.getHealthLog().then(
      (h) => (health = h),
      (e) => toasts.push(errorText(e)),
    )
  })

  const DESCRIPTION: Record<BatteryProfile, string> = {
    home: 'Holds 75–80%, ideal when plugged in at a desk',
    campus: 'Charges to 100% before you head out',
    storage: '50–60% for long-term storage',
  }

  const app = $derived(live.app)
  const active = $derived(app?.activeProfile?.kind === 'known' ? app.activeProfile.profile : null)
  const battery = $derived(live.telemetry?.battery ?? live.app?.battery ?? null)
  const pctPoints = $derived(samples.map((s) => ({ t: s.tsMs, v: s.batteryPct })))
  const wPoints = $derived(samples.map((s) => ({ t: s.tsMs, v: s.batteryW })))

  const timeLeft = $derived(battery?.state === 'discharging' ? timeLeftMin(samples, battery.remainingMwh) : null)

  // Daily capacity as bars, relative to the factory capacity.
  const healthMax = $derived(Math.max(...health.map((h) => h.designMwh), 1))
</script>

<div class="stack">
  <Card>
    <ModeControls thermal={false} disabled={app ? !app.availability.cctk : false} />
    <ul>
      {#each Object.keys(PROFILE_LABEL) as BatteryProfile[] as p (p)}
        <li class:active={active === p}>
          <span class="name">{PROFILE_LABEL[p]}</span>
          <span class="secondary">{DESCRIPTION[p]}</span>
          <span class="secondary num">{app ? chargeCfgLabel(app.profiles[p]) : '—'}</span>
        </li>
      {/each}
    </ul>
  </Card>

  <Card title="Health">
    <dl>
      <div><dt>Factory capacity</dt><dd class="num">{wh(battery?.designMwh)}</dd></div>
      <div><dt>Current capacity</dt><dd class="num">{wh(battery?.fullMwh)}</dd></div>
      <div><dt>Wear</dt><dd class="num">{pct(battery?.wearPct, 1)}</dd></div>
      {#if battery?.cycles != null}
        <div><dt>Cycles</dt><dd class="num">{num(battery.cycles)}</dd></div>
      {/if}
      <div><dt>Voltage</dt><dd class="num">{num(battery?.voltageV, 2, ' V')}</dd></div>
      <div><dt>Power</dt><dd class="num">{watts(battery?.powerW)}</dd></div>
      {#if battery?.state === 'discharging'}
        <div><dt>Time left</dt><dd class="num">{timeLeft == null ? 'Estimating…' : duration(timeLeft)}</dd></div>
      {/if}
    </dl>
  </Card>

  <Card title="Capacity history">
    {#if health.length}
      <div class="bars" role="img" aria-label="Full charge capacity per day">
        {#each health as h (h.date)}
          <div class="bar" style:height="{(h.fullMwh / healthMax) * 100}%" title="{h.date}: {wh(h.fullMwh)}"></div>
        {/each}
      </div>
      <div class="range secondary num">
        <span>{health[0].date}</span>
        <span>{health.at(-1)?.date}</span>
      </div>
    {:else}
      <p class="secondary">Recorded once a day — check back tomorrow.</p>
    {/if}
  </Card>

  <Card title="Charge — last {HISTORY_MINUTES} minutes">
    <LineChart samples={pctPoints} minutes={HISTORY_MINUTES} unit="%" min={0} max={100} />
  </Card>

  <Card title="Power — last {HISTORY_MINUTES} minutes">
    <LineChart samples={wPoints} minutes={HISTORY_MINUTES} unit=" W" />
  </Card>
</div>

<style>
  .stack {
    display: grid;
    gap: 12px;
    max-width: 880px;
  }

  ul {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }

  li {
    display: grid;
    grid-template-columns: 80px 1fr auto;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--r-ctl);
    font-size: 13px;
  }

  li.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .name {
    font-weight: 600;
  }

  dl {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px 18px;
  }

  dt {
    font-size: 12px;
    color: var(--text-2);
  }

  dd {
    margin: 2px 0 0;
    font-size: 18px;
    font-weight: 500;
  }

  .bars {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 100px;
  }

  .bar {
    flex: 1;
    min-width: 2px;
    max-width: 16px;
    background: var(--accent);
    border-radius: 2px 2px 0 0;
  }

  .range {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    font-size: 11px;
  }

  p {
    margin: 0;
  }
</style>
