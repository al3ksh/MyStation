<script lang="ts" module>
  import type { BiosGroup, BiosSetting } from '../../lib/types'

  // A read costs several seconds, so it survives switching sections.
  let cache: BiosSetting[] | null = null

  /** Drops the cached read; tests start each case fresh. */
  export function forget() {
    cache = null
  }

  const GROUPS: BiosGroup[] = ['Keyboard', 'Power', 'Devices', 'Startup']
</script>

<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Switch from '../../components/Switch.svelte'
  import { api } from '../../lib/api'
  import { errorText, toasts } from '../../lib/toasts.svelte'

  let settings = $state<BiosSetting[] | null>(cache)
  let loadError = $state<string | null>(null)
  let loading = $state(false)
  /** The change waiting for the user's confirmation. */
  let pending = $state<{ setting: BiosSetting; value: string } | null>(null)
  /** Key of the setting being written. */
  let writing = $state<string | null>(null)

  async function load() {
    loading = true
    loadError = null
    try {
      settings = cache = await api.getBiosSettings()
    } catch (e) {
      if (!settings) loadError = errorText(e)
      else toasts.push(errorText(e))
    } finally {
      loading = false
    }
  }

  onMount(() => {
    if (!settings) load()
  })

  function shown(s: BiosSetting, value: string | null): string {
    if (value === null) return 'Unknown'
    if (s.kind === 'toggle') return value === 'Enabled' ? 'On' : value === 'Disabled' ? 'Off' : value
    if (s.kind === 'choice') return s.choices.find((c) => c.value === value)?.label ?? value
    return value
  }

  function visible(s: BiosSetting): boolean {
    return !s.parent || settings?.find((p) => p.key === s.parent)?.value !== 'Disabled'
  }

  function ask(setting: BiosSetting, value: string) {
    if (value !== setting.value) pending = { setting, value }
  }

  async function confirm() {
    if (!pending) return
    const { setting, value } = pending
    pending = null
    writing = setting.key
    try {
      await api.setBiosSetting(setting.key, value)
      const s = settings?.find((x) => x.key === setting.key)
      if (s) s.value = value
    } catch (e) {
      toasts.push(errorText(e))
      await load()
    } finally {
      writing = null
    }
  }

  function range(min: number, max: number): string[] {
    return Array.from({ length: max - min + 1 }, (_, i) => String(min + i))
  }

  function focus(el: HTMLElement) {
    el.focus()
  }
</script>

{#snippet control(s: BiosSetting)}
  {@const busy = writing !== null}
  {#if s.kind === 'toggle'}
    <Switch
      checked={s.value === 'Enabled'}
      label={s.label}
      disabled={busy || (s.value !== 'Enabled' && s.value !== 'Disabled')}
      onchange={() => ask(s, s.value === 'Enabled' ? 'Disabled' : 'Enabled')}
    />
  {:else}
    {@const options =
      s.kind === 'choice' ? s.choices : range(s.min, s.max).map((v) => ({ value: v, label: v }))}
    <select
      aria-label={s.label}
      value={s.value ?? ''}
      disabled={busy}
      onchange={(e) => {
        const value = e.currentTarget.value
        // The select shows the BIOS value until the change is confirmed and written.
        e.currentTarget.value = s.value ?? ''
        ask(s, value)
      }}
    >
      {#if !options.some((o) => o.value === s.value)}
        <option value={s.value ?? ''} disabled>{shown(s, s.value)}</option>
      {/if}
      {#each options as o (o.value)}
        <option value={o.value}>{o.label}</option>
      {/each}
    </select>
  {/if}
{/snippet}

<div class="stack">
  <div class="note">
    <Icon name="info" />
    <span class="secondary">Changes are written to the BIOS right away. Some apply only after a restart.</span>
    {#if settings}
      <button type="button" class="btn" disabled={loading || writing !== null} onclick={load}>
        <Icon name="refresh" />
        {loading ? 'Reading…' : 'Refresh'}
      </button>
    {/if}
  </div>

  {#if settings}
    {#each GROUPS as group (group)}
      {@const rows = settings.filter((s) => s.group === group && visible(s))}
      {#if rows.length}
        <section>
          <h2>{group}</h2>
          <div class="card">
            {#each rows as s (s.key)}
              <div class="row" class:child={s.parent}>
                <div class="text">
                  <span>{s.label}</span>
                  <span class="secondary">{writing === s.key ? 'Writing to the BIOS…' : s.description}</span>
                </div>
                {@render control(s)}
              </div>
            {/each}
          </div>
        </section>
      {/if}
    {/each}
  {:else if loadError}
    <div class="state">
      <p>{loadError}</p>
      <button type="button" class="btn" onclick={load}><Icon name="refresh" /> Try again</button>
    </div>
  {:else}
    <div class="state" role="status">
      <span class="spinner"></span>
      <p class="secondary">Reading BIOS settings… This takes a few seconds.</p>
    </div>
  {/if}
</div>

{#if pending}
  {@const p = pending}
  <div class="scrim">
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="bios-confirm-title"
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === 'Escape') pending = null
      }}
    >
      <h2 id="bios-confirm-title">Change {p.setting.label}?</h2>
      <p>
        From <strong>{shown(p.setting, p.setting.value)}</strong> to <strong>{shown(p.setting, p.value)}</strong>.
      </p>
      <p class="secondary">This writes to the BIOS. Some settings apply only after a restart.</p>
      <div class="actions">
        <button type="button" class="btn accent" onclick={confirm}>Change</button>
        <button type="button" class="btn" use:focus onclick={() => (pending = null)}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .stack {
    display: grid;
    gap: 20px;
    max-width: 880px;
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

  h2 {
    margin: 0 0 8px;
    font-size: 14px;
    font-weight: 600;
  }

  .card {
    display: grid;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 60px;
    padding: 10px 16px;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row.child {
    padding-left: 40px;
  }

  .text {
    display: grid;
    gap: 2px;
    margin-right: auto;
  }

  .text .secondary {
    font-size: 12px;
  }

  select {
    min-width: 160px;
    height: 32px;
    padding: 0 10px;
    font: inherit;
    color: inherit;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
  }

  select option {
    background: var(--solid);
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

  .btn.accent {
    background: var(--accent);
    border-color: transparent;
    color: var(--on-accent);
  }

  .btn.accent:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .state {
    display: grid;
    justify-items: center;
    gap: 12px;
    padding: 48px 0;
  }

  .state p {
    margin: 0;
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

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.35);
  }

  .dialog {
    width: min(440px, calc(100% - 48px));
    padding: 24px;
    background: var(--solid);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
    box-shadow: 0 8px 32px rgb(0 0 0 / 0.3);
  }

  .dialog h2 {
    font-size: 18px;
    margin-bottom: 12px;
  }

  .dialog p {
    margin: 0 0 8px;
  }

  .dialog .secondary {
    font-size: 13px;
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 20px;
  }
</style>
