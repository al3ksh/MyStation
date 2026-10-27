<script lang="ts" module>
  import type { MemoryModule, PciLink, Spec } from '../../lib/types'

  // The hardware doesn't change while the app runs; read it once.
  let cache: Spec | null = null

  /** Drops the cached read; tests start each case fresh. */
  export function forget() {
    cache = null
  }

  const round = (v: number) => +v.toFixed(1)
  const mb = (v: number) => (v >= 1024 ? `${round(v / 1024)} GB` : `${v} MB`)
  const gb = (v: number) => (v >= 1000 ? `${round(v / 1000)} TB` : `${v} GB`)
  const pcie = (l: PciLink | null) => (l ? `PCIe ${l.generation}.0 x${l.lanes}` : null)
  const join = (parts: (string | null | false)[]) => parts.filter(Boolean).join(' · ')

  function moduleText(m: MemoryModule): string {
    let kind = [mb(m.sizeMb), m.kind && (m.speedMts ? `${m.kind}-${m.speedMts}` : m.kind)].filter(Boolean).join(' ')
    if (m.speedMts && m.configuredMts && m.configuredMts < m.speedMts) kind += `, runs at ${m.configuredMts}`
    return join([kind, [m.maker, m.part].filter(Boolean).join(' ')])
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { wh } from '../../lib/format'
  import { live } from '../../lib/telemetry.svelte'
  import { errorText } from '../../lib/toasts.svelte'

  let spec = $state<Spec | null>(cache)
  let error = $state<string | null>(null)

  const designMwh = $derived(live.telemetry?.battery?.designMwh ?? live.app?.battery?.designMwh ?? null)
  const installedMb = $derived(spec?.memory.modules.reduce((sum, m) => sum + m.sizeMb, 0) ?? 0)

  onMount(() => {
    if (!spec) api.getSpec().then((s) => (spec = cache = s), (e) => (error = errorText(e)))
  })
</script>

<div class="stack">
  <div class="note">
    <Icon name="info" />
    <span class="secondary">What this laptop has and supports, read from the laptop itself.</span>
  </div>

  {#if spec}
    {@const s = spec}
    <section>
      <h2>System</h2>
      <div class="card">
        <dl>
          {#if s.model}<dt>Model</dt><dd>{s.model}</dd>{/if}
          {#if s.biosVersion}<dt>BIOS</dt><dd>{join([s.biosVersion, s.biosDate])}</dd>{/if}
          {#if designMwh}<dt>Battery</dt><dd>{wh(designMwh)} when new</dd>{/if}
        </dl>
      </div>
    </section>

    {#if s.cpu}
      <section>
        <h2>Processor</h2>
        <div class="card">
          <p>{s.cpu.name}</p>
          <dl>
            <dt>Cores</dt><dd>{s.cpu.cores} cores, {s.cpu.threads} threads</dd>
            {#if s.cpu.l2Kb || s.cpu.l3Kb}
              <dt>Cache</dt><dd>{join([s.cpu.l2Kb > 0 && `L2 ${mb(s.cpu.l2Kb / 1024)}`, s.cpu.l3Kb > 0 && `L3 ${mb(s.cpu.l3Kb / 1024)}`])}</dd>
            {/if}
            {#if s.cpu.features.length}<dt>Instructions</dt><dd>{s.cpu.features.join(' · ')}</dd>{/if}
            {#if s.cpu.virtualization != null}
              <dt>Virtualization</dt><dd>{s.cpu.virtualization ? 'Enabled' : 'Disabled'}</dd>
            {/if}
          </dl>
        </div>
      </section>
    {/if}

    {#if s.memory.modules.length || s.memory.maxGb}
      <section>
        <h2>Memory</h2>
        <div class="card">
          <dl>
            <dt>Installed</dt>
            <dd>{s.memory.maxGb ? `${mb(installedMb)} of ${s.memory.maxGb} GB` : mb(installedMb)}</dd>
            {#if s.memory.slots}<dt>Slots</dt><dd>{s.memory.modules.length} of {s.memory.slots} used</dd>{/if}
            {#each s.memory.modules as m (m.slot)}
              <dt>{m.slot}</dt><dd>{moduleText(m)}</dd>
            {/each}
          </dl>
        </div>
      </section>
    {/if}

    {#if s.graphics.length}
      <section>
        <h2>Graphics</h2>
        <div class="card list">
          {#each s.graphics as g (g.name)}
            {@const detail = join([g.vramMb != null && mb(g.vramMb), pcie(g.link), g.driver && `Driver ${g.driver}`])}
            <div class="row">
              <div class="text">
                <span>{g.name}</span>
                {#if detail}<span class="secondary">{detail}</span>{/if}
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if s.storage.length}
      <section>
        <h2>Storage</h2>
        <div class="card list">
          {#each s.storage as d (d.name)}
            {@const detail = join([d.sizeGb != null && gb(d.sizeGb), pcie(d.link)])}
            <div class="row">
              <div class="text">
                <span>{d.name}</span>
                {#if detail}<span class="secondary">{detail}</span>{/if}
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if s.panel}
      <section>
        <h2>Display</h2>
        <div class="card">
          <dl>
            <dt>Screen</dt>
            <dd>{join([s.panel.diagonalIn != null && `${s.panel.diagonalIn}″`, `${s.panel.width} × ${s.panel.height}`, `up to ${s.panel.maxHz} Hz`])}</dd>
            {#if s.panel.maker}<dt>Panel</dt><dd>{s.panel.maker}</dd>{/if}
          </dl>
        </div>
      </section>
    {/if}

    {#if s.wifi || s.bluetooth || s.thunderbolt}
      <section>
        <h2>Connectivity</h2>
        <div class="card">
          <dl>
            {#if s.wifi}<dt>Wi-Fi</dt><dd>{s.wifi}</dd>{/if}
            {#if s.bluetooth}<dt>Bluetooth</dt><dd>Version {s.bluetooth}</dd>{/if}
            {#if s.thunderbolt}<dt>Ports</dt><dd>{s.thunderbolt}</dd>{/if}
          </dl>
        </div>
      </section>
    {/if}

    {#if s.tpm || s.secureBoot != null || s.fingerprint}
      <section>
        <h2>Security</h2>
        <div class="card">
          <dl>
            {#if s.tpm}<dt>TPM</dt><dd>{s.tpm}</dd>{/if}
            {#if s.secureBoot != null}<dt>Secure Boot</dt><dd>{s.secureBoot ? 'On' : 'Off'}</dd>{/if}
            {#if s.fingerprint}<dt>Fingerprint reader</dt><dd>{s.fingerprint}</dd>{/if}
          </dl>
        </div>
      </section>
    {/if}
  {:else if error}
    <p class="secondary">{error}</p>
  {:else}
    <div class="state" role="status"><span class="spinner"></span></div>
  {/if}
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

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
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

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
    font-size: 13px;
  }

  dt {
    color: var(--text-secondary);
  }

  dd {
    margin: 0;
  }

  .list {
    gap: 0;
    padding: 0;
  }

  .row {
    display: flex;
    align-items: center;
    min-height: 52px;
    padding: 8px 16px;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .text {
    display: grid;
    gap: 2px;
  }

  .text .secondary {
    font-size: 12px;
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
