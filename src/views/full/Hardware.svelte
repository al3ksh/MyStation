<script lang="ts" module>
  import type { BootReport, Culprit, DriveReport, SmartLog, StorageReport, UsbDevice, UsbReport } from '../../lib/types'

  // Both reads touch the disk and the event log; keep them across section switches.
  let storageCache: StorageReport | null = null
  let bootCache: BootReport | null = null

  /** Drops the cached reads; tests start each case fresh. */
  export function forget() {
    storageCache = null
    bootCache = null
  }

  const KIND: Record<Culprit['kind'], string> = { app: 'App', driver: 'Driver', service: 'Service', device: 'Device' }
</script>

<script lang="ts">
  import { onMount } from 'svelte'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { num, pct, temp } from '../../lib/format'
  import { errorText, toasts } from '../../lib/toasts.svelte'

  let storage = $state<StorageReport | null>(storageCache)
  let boot = $state<BootReport | null>(bootCache)
  let storageError = $state<string | null>(null)
  let bootError = $state<string | null>(null)
  // Devices come and go, so they are read again every time the page opens.
  let usb = $state<UsbReport | null>(null)
  let loading = $state(false)

  const loadUsb = () => api.getUsb().then((u) => (usb = u), (e) => toasts.push(errorText(e)))

  async function load() {
    loading = true
    storageError = bootError = null
    await Promise.all([
      loadUsb(),
      api.getStorage().then(
        (s) => (storage = storageCache = s),
        (e) => (storage ? toasts.push(errorText(e)) : (storageError = errorText(e))),
      ),
      api.getBoot().then(
        (b) => (boot = bootCache = b),
        (e) => (boot ? toasts.push(errorText(e)) : (bootError = errorText(e))),
      ),
    ])
    loading = false
  }

  onMount(() => {
    if (!storage || !boot) load()
    else loadUsb()
  })

  function bytes(b: number): string {
    return b >= 1e12 ? num(b / 1e12, 1, ' TB') : num(b / 1e9, 0, ' GB')
  }

  const secs = (ms: number) => num(ms / 1000, 1, ' s')

  function problem(s: SmartLog): string | null {
    if (s.criticalWarning !== 0) return 'The drive reports a critical warning. Back up your data now.'
    if (s.availableSparePct < s.spareThresholdPct) return 'Spare capacity is below the safe level. Back up your data.'
    if (s.mediaErrors > 0) return `${num(s.mediaErrors)} unrecovered read errors recorded.`
    return null
  }

  function forecast(d: DriveReport, s: SmartLog): string {
    if (s.percentUsed >= 100) return 'Past its rated endurance. It may keep working, but plan a replacement.'
    if (d.yearsLeft != null) {
      if (d.yearsLeft >= 20) return 'More than 20 years left at your current write rate.'
      return `About ${num(d.yearsLeft, d.yearsLeft < 3 ? 1 : 0)} years left at your current write rate.`
    }
    if (s.percentUsed === 0) return 'Less than 1% of rated endurance used.'
    const since = d.trackingSince ? ` (tracking since ${date(d.trackingSince)})` : ''
    return `A lifetime forecast appears after a week of tracking${since}.`
  }

  function date(iso: string, time = false): string {
    const d = new Date(iso)
    return time
      ? d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
      : d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
  }

  function draw(d: UsbDevice): string {
    if (d.drawW == null) return 'Not measured'
    return d.drawW < 0.1 ? 'Under 0.1 W' : `+${num(d.drawW, 1, ' W')}`
  }

  function usbDetail(d: UsbDevice): string {
    const power = d.suspended ? 'Sleeps when idle' : 'Always on'
    return d.arrivedMs == null ? power : `Plugged in ${date(new Date(d.arrivedMs).toISOString(), true)} · ${power}`
  }

  function builtIn(n: number): string {
    return n === 1 ? "1 built-in device, like the camera, isn't listed." : `${n} built-in devices, like the camera, aren't listed.`
  }

  const slowest = $derived(boot ? Math.max(1, ...boot.boots.map((b) => b.totalMs)) : 1)
</script>

<div class="stack">
  <div class="note">
    <Icon name="info" />
    <span class="secondary">Drive health comes from the SSD's own records. Boot times come from Windows.</span>
    {#if storage || boot}
      <button type="button" class="btn" disabled={loading} onclick={load}>
        <Icon name="refresh" />
        {loading ? 'Reading…' : 'Refresh'}
      </button>
    {/if}
  </div>

  <section>
    <h2>Storage</h2>
    {#if storage}
      {#each storage.drives as d, i (i)}
        {@const s = d.smart}
        <div class="card">
          <div class="head">
            <Icon name="drive" />
            <strong>{d.model ?? 'Drive'}</strong>
            {#if s?.tempC != null}<span class="secondary right">{temp(s.tempC)}</span>{/if}
          </div>
          {#if s}
            {@const issue = problem(s)}
            <p class="status" class:bad={issue}>{issue ?? 'Healthy'}</p>
            <div class="wear">
              <div class="label">
                <span>Wear</span>
                <span class="secondary">{pct(s.percentUsed)} of rated endurance</span>
              </div>
              <div class="bar" role="meter" aria-label="Wear" aria-valuenow={s.percentUsed} aria-valuemin={0} aria-valuemax={100}>
                <span style:width="{Math.min(100, Math.max(1, s.percentUsed))}%"></span>
              </div>
              <p class="secondary">{forecast(d, s)}</p>
            </div>
            <dl>
              <dt>Written</dt><dd>{bytes(s.bytesWritten)}</dd>
              <dt>Read</dt><dd>{bytes(s.bytesRead)}</dd>
              <dt>Powered on</dt><dd>{num(s.powerOnHours, 0, ' h')}</dd>
              <dt>Power cycles</dt><dd>{num(s.powerCycles)}</dd>
              <dt>Unsafe shutdowns</dt><dd>{num(s.unsafeShutdowns)}</dd>
              <dt>Spare capacity</dt><dd>{pct(s.availableSparePct)}</dd>
            </dl>
          {:else}
            <p class="secondary">{d.nvme ? 'Health data needs administrator rights.' : 'Health data is available for NVMe drives only.'}</p>
          {/if}
        </div>
      {:else}
        <div class="card"><p class="secondary">No drives found.</p></div>
      {/each}
      {#if storage.volume}
        {@const [total, free] = storage.volume}
        <p class="secondary volume">Windows (C:): {bytes(free)} free of {bytes(total)}</p>
      {/if}
    {:else if storageError}
      <div class="card"><p>{storageError}</p></div>
    {:else}
      <div class="state" role="status"><span class="spinner"></span></div>
    {/if}
  </section>

  <section>
    <h2>Startup</h2>
    {#if boot}
      {@const last = boot.boots[0]}
      <div class="card">
        {#if last}
          <div class="head">
            <span class="big">{secs(last.totalMs)}</span>
            <span class="secondary">last boot, {date(last.start, true)}</span>
          </div>
          <p class="secondary">
            {secs(last.mainPathMs)} to the desktop, then {secs(last.postBootMs)} for {num(last.startupApps)} startup apps
            and services to settle.
          </p>
          {#if boot.boots.length > 1}
            <ul class="boots" aria-label="Recent boots">
              {#each boot.boots as b (b.start)}
                <li>
                  <span class="secondary">{date(b.start)}</span>
                  <span class="bar"><span style:width="{(b.totalMs / slowest) * 100}%"></span></span>
                  <span>{secs(b.totalMs)}</span>
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          <p class="secondary">No full boots recorded yet. Waking from sleep or hibernation doesn't count as a boot.</p>
        {/if}
      </div>

      {#if boot.culprits.length}
        <h3>What slowed it down</h3>
        <div class="card list">
          {#each boot.culprits as c (c.kind + c.name)}
            <div class="row">
              <div class="text">
                <span>{c.name}</span>
                <span class="secondary">{KIND[c.kind]} · slowed {c.boots} of the last {boot.boots.length} boots</span>
              </div>
              <span>+{secs(c.avgDelayMs)}</span>
            </div>
          {/each}
        </div>
      {/if}
      <button type="button" class="btn link" onclick={() => openUrl('ms-settings:startupapps')}>
        <Icon name="openWindow" /> Manage startup apps
      </button>
    {:else if bootError}
      <div class="card"><p>{bootError}</p></div>
    {:else}
      <div class="state" role="status"><span class="spinner"></span></div>
    {/if}
  </section>

  <section>
    <h2>USB devices</h2>
    {#if usb}
      <div class="card list usb">
        {#each usb.devices as d, i (i)}
          <div class="row">
            <div class="text">
              <span>{d.name}</span>
              <span class="secondary">{usbDetail(d)}</span>
            </div>
            <span class:secondary={d.drawW == null || d.drawW < 0.1}>{draw(d)}</span>
          </div>
        {:else}
          <div class="row"><span class="secondary">Nothing is plugged in.</span></div>
        {/each}
      </div>
      <p class="secondary hint">
        To measure a device, plug it in while on battery with this window open, wait a minute, then refresh.
        {#if usb.builtIn}{builtIn(usb.builtIn)}{/if}
      </p>
    {:else}
      <div class="state" role="status"><span class="spinner"></span></div>
    {/if}
  </section>
</div>

<style>
  .stack {
    display: grid;
    gap: 20px;
    max-width: 880px;
  }

  section {
    display: grid;
    gap: 8px;
  }

  .note {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .note .btn {
    margin-left: auto;
  }

  h2,
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  h3 {
    margin-top: 8px;
    font-size: 13px;
  }

  .card {
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .card p {
    margin: 0;
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .right {
    margin-left: auto;
  }

  .big {
    font-size: 28px;
    font-weight: 600;
  }

  .status {
    color: var(--ok, inherit);
  }

  .status.bad {
    color: var(--danger, #e81123);
  }

  .wear {
    display: grid;
    gap: 6px;
  }

  .label {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
  }

  .bar {
    display: block;
    height: 6px;
    overflow: hidden;
    background: var(--surface-hover);
    border-radius: 3px;
  }

  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
  }

  dl {
    display: grid;
    grid-template-columns: repeat(3, auto 1fr);
    gap: 6px 12px;
    margin: 0;
    font-size: 13px;
  }

  dt {
    color: var(--text-secondary);
  }

  dd {
    margin: 0;
  }

  .volume,
  .hint {
    margin: 0;
    font-size: 13px;
  }

  .boots {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }

  .boots li {
    display: grid;
    grid-template-columns: 110px 1fr 60px;
    align-items: center;
    gap: 12px;
  }

  .boots li > span:last-child {
    text-align: right;
  }

  .list {
    gap: 0;
    padding: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 52px;
    padding: 8px 16px;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .text {
    display: grid;
    gap: 2px;
    margin-right: auto;
  }

  .text .secondary {
    font-size: 12px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
    color: inherit;
    font: inherit;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
    cursor: default;
  }

  .btn:hover:not(:disabled) {
    background: var(--surface-raised);
  }

  .btn:disabled {
    opacity: 0.5;
  }

  .btn.link {
    justify-self: start;
  }

  .state {
    display: grid;
    justify-items: center;
    padding: 32px 0;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
