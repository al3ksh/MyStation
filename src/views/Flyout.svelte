<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import AnimatedNumber from '../components/AnimatedNumber.svelte'
  import Banner from '../components/Banner.svelte'
  import Icon from '../components/Icon.svelte'
  import ModeControls from '../components/ModeControls.svelte'
  import Toasts from '../components/Toasts.svelte'
  import { api } from '../lib/api'
  import { bannersFor } from '../lib/banners'
  import { appName, FALLBACK_NAME } from '../lib/brand'
  import { num, pct, temp, watts } from '../lib/format'
  import type { IconName } from '../lib/icons'
  import { live, startWindow } from '../lib/telemetry.svelte'
  import { errorText, toasts } from '../lib/toasts.svelte'
  import type { BatterySnapshot, PeripheralKind } from '../lib/types'

  let root: HTMLElement
  let name = $state(FALLBACK_NAME)

  /** The window is sized to the content, so it never shows empty space or a scrollbar. */
  const fit = async (height: number) => {
    if (height > 0) await api.fitFlyout(Math.ceil(height)).catch(() => {})
  }

  onMount(() => {
    void appName().then((n) => (name = n))
    // The flyout opens on the app state with a skeleton for the sensors; telemetry fills it in place.
    // It shows only once sized, so it never appears at a stale height and then jumps.
    void startWindow((e) => toasts.push(errorText(e)), () => fit(root.getBoundingClientRect().height), {
      waitForTick: false,
    })
    // The resize observer is paused while the window is hidden: measure again on every show.
    const onShow = () => {
      if (document.visibilityState === 'visible') void fit(root.getBoundingClientRect().height)
    }
    document.addEventListener('visibilitychange', onShow)
    if (typeof ResizeObserver === 'undefined') return () => document.removeEventListener('visibilitychange', onShow)
    const ro = new ResizeObserver(([e]) => fit(e.borderBoxSize[0].blockSize))
    ro.observe(root)
    return () => {
      ro.disconnect()
      document.removeEventListener('visibilitychange', onShow)
    }
  })
  onDestroy(() => live.stop())

  const app = $derived(live.app)

  /** Matches `bluetooth::LOW_PCT`, where the low-battery notification fires. */
  const LOW_PCT = 15
  const KIND_ICON: Record<PeripheralKind, IconName> = {
    mouse: 'mouse',
    keyboard: 'keyboard',
    headphones: 'headphones',
    speaker: 'speaker',
    controller: 'controller',
    pen: 'pen',
    phone: 'phone',
    other: 'bluetooth',
  }
  // The flyout has room for one banner: the most severe.
  const banner = $derived(app ? bannersFor(app)[0] : undefined)
  const t = $derived(live.telemetry)
  // Telemetry is fresher (1 s); the app state covers the first paint.
  const battery = $derived<BatterySnapshot | null>(t?.battery ?? app?.battery ?? null)

  const power = $derived.by((): [string, string] => {
    const b = battery
    if (!b) return ['—', '']
    if (b.state === 'charging') return ['Charging', watts(b.powerW)]
    if (b.state === 'discharging') return ['On battery', watts(b.powerW)]
    return ['Plugged in', 'Battery bypassed']
  })
  const batteryIcon = $derived<IconName>(battery?.state === 'discharging' ? 'battery' : 'batteryCharging')

  interface Row {
    icon: IconName
    name: string
    /** Null until the first telemetry tick: drawn as a skeleton of the same size. */
    value: string | null
    sub?: string
    /** 0–100, drawn as a thin bar under the row; null leaves the track empty. */
    bar?: number | null
    /** A Bluetooth battery at or under LOW_PCT: value and bar in red. */
    low?: boolean
  }

  // Connected Bluetooth devices sit under the sensors, emptiest first.
  const devices = $derived(
    (app?.peripherals ?? []).map(
      (d): Row => ({
        icon: KIND_ICON[d.kind],
        name: d.name,
        value: `${d.batteryPct}%`,
        bar: d.batteryPct,
        low: d.batteryPct <= LOW_PCT,
      }),
    ),
  )

  const rows = $derived.by((): Row[] => [...sensors(), ...devices])

  function sensors(): Row[] {
    if (!t) {
      return [
        { icon: 'cpu', name: 'CPU', value: null, bar: null },
        { icon: 'gpu', name: 'GPU', value: null, bar: null },
        { icon: 'fan', name: 'Fans', value: null },
        { icon: 'memory', name: 'Memory', value: null, bar: null },
      ]
    }
    const g = t.gpu
    const gpu: Row =
      g.state === 'unavailable'
        ? { icon: 'gpu', name: 'GPU', value: '—', bar: null }
        : g.state === 'asleep'
          ? { icon: 'gpu', name: 'GPU', value: 'Asleep', bar: null }
          : { icon: 'gpu', name: 'GPU', value: temp(g.tempC), sub: pct(g.loadPct), bar: g.loadPct }
    const fans = t.fans.length ? t.fans.map((f) => num(f.rpm)).join(' · ') : '—'
    const mem = t.mem
    return [
      { icon: 'cpu', name: 'CPU', value: temp(t.cpu.tempC), sub: pct(t.cpu.loadPct), bar: t.cpu.loadPct },
      gpu,
      { icon: 'fan', name: 'Fans', value: fans, sub: t.fans.length ? 'RPM' : undefined },
      {
        icon: 'memory',
        name: 'Memory',
        value: mem ? `${num(mem.usedMb / 1024, 1)} / ${num(mem.totalMb / 1024, 1)} GB` : '—',
        bar: mem && mem.totalMb > 0 ? (mem.usedMb / mem.totalMb) * 100 : null,
      },
    ]
  }
</script>

<div class="flyout" bind:this={root}>
  <div class="body">
    <header>
      <Icon name={batteryIcon} size={28} />
      <span class="big num">
        {#if battery?.percent != null}
          <AnimatedNumber value={battery.percent} suffix="%" />
        {:else}
          —
        {/if}
      </span>
      <span class="power secondary num"><span>{power[0]}</span><span>{power[1]}</span></span>
    </header>

    {#if banner}
      <Banner id={banner} />
    {/if}

    <ModeControls disabled={app ? !app.availability.cctk : false} />

    <section>
      <h2>Sensors</h2>
      <ul>
        {#each rows as r (r.name)}
          <li class:low={r.low}>
            <Icon name={r.icon} />
            <span class="name" title={r.name}>{r.name}</span>
            {#if r.value === null}
              <span class="skeleton" aria-label="Loading"></span>
            {:else}
              <span class="num"><span>{r.value}</span>{#if r.sub}<span class="secondary">{' · '}{r.sub}</span>{/if}</span>
            {/if}
            {#if r.bar !== undefined}
              <span class="bar"><span style:width="{Math.min(100, Math.max(0, r.bar ?? 0))}%"></span></span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>

    <Toasts />
  </div>

  <footer>
    <Icon name="gauge" size={14} />
    <span>{name}</span>
    <button type="button" class="icon-btn" aria-label="Open full window" title="Open full window" onclick={() => api.openFullWindow()}>
      <Icon name="openWindow" size={14} />
    </button>
  </footer>
</div>

<style>
  .flyout {
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-flyout);
    overflow: hidden;
  }

  /* One line of text tall, so the value that replaces it doesn't move the rows. */
  .skeleton {
    width: 72px;
    height: 1lh;
    border-radius: 4px;
    background: linear-gradient(90deg, var(--surface-hover) 25%, var(--surface) 50%, var(--surface-hover) 75%) 0 0 / 200% 100%;
    animation: shimmer 1.2s linear infinite;
  }

  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  header > :global(.icon) {
    color: var(--accent);
  }

  .big {
    font-size: 36px;
    font-weight: 300;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .power {
    display: grid;
    margin-left: auto;
    text-align: right;
    font-size: 12px;
    line-height: 1.4;
  }

  h2 {
    margin: 0 0 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 20px 1fr auto;
    align-items: center;
    column-gap: 10px;
    row-gap: 5px;
    padding: 6px 0;
    font-size: 13px;
  }

  .bar {
    grid-column: 2 / 4;
    height: 3px;
    border-radius: 2px;
    background: var(--surface-hover);
    overflow: hidden;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .low .num {
    color: var(--danger, #e81123);
  }

  .low .bar span {
    background: var(--danger, #e81123);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 400ms var(--ease);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 16px;
    font-size: 12px;
    color: var(--text-2);
    background: var(--sunk);
    border-top: 1px solid var(--border);
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-left: auto;
    background: none;
    border: none;
    border-radius: var(--r-ctl);
    color: var(--text);
    cursor: default;
  }

  .icon-btn:hover {
    background: var(--surface-hover);
  }
</style>
