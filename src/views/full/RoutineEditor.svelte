<script lang="ts">
  import { untrack } from 'svelte'
  import ChipGroup from '../../components/ChipGroup.svelte'
  import Icon from '../../components/Icon.svelte'
  import { POWER_MODE_LABEL, PROFILE_LABEL, THERMAL_LABEL, powerModeFor } from '../../lib/labels'
  import {
    CONDITION_KINDS,
    DAYS,
    NAME_MAX,
    POWER_MODE_SHORT,
    defaultCondition,
    defaultName,
    isInstant,
    isManual,
    routineProblem,
  } from '../../lib/routines'
  import { live } from '../../lib/telemetry.svelte'
  import type {
    BatteryProfile,
    ConditionKind,
    PowerMode,
    Routine,
    RoutineOptions,
    ThermalMode,
  } from '../../lib/types'

  interface Props {
    routine: Routine
    isNew: boolean
    options: RoutineOptions
    onsave: (r: Routine) => void
    oncancel: () => void
    ondelete: () => void
  }

  let { routine, isNew, options, onsave, oncancel, ondelete }: Props = $props()

  const KEEP = ''
  const OTHER = '\u0000other'
  const MODES = Object.keys(THERMAL_LABEL) as ThermalMode[]
  const PROFILES = Object.keys(PROFILE_LABEL) as BatteryProfile[]
  const POWER_MODES = Object.keys(POWER_MODE_SHORT) as PowerMode[]
  const PERCENTS = [10, 15, 20, 30]
  /** What each condition's chips are about. */
  const CONDITION_LABEL: Record<ConditionKind, string> = {
    wifi: 'Wi-Fi network',
    power: 'Power source',
    time: 'Time',
    display: 'External display',
    bluetooth: 'Bluetooth device',
    batteryBelow: 'Battery below',
    longAc: 'Plugged in without a break for',
  }
  const AC_DAYS = [1, 3, 7, 14]

  // The editor works on its own copy, taken once; nothing changes until Save.
  let draft = $state(untrack(() => $state.snapshot(routine)) as Routine)
  // A new routine is named after its conditions until a name is typed in.
  let named = $state(untrack(() => !isNew))
  // "Other" picked: show the field even while what it holds is one of the listed ones.
  let otherSsid = $state(false)
  let otherDevice = $state(false)
  let otherPct = $state(false)
  let otherDays = $state(false)

  const shownName = $derived(named ? draft.name : defaultName(draft))
  const problem = $derived(routineProblem(draft))
  const manual = $derived(isManual(draft))
  const instant = $derived(isInstant(draft))
  const networks = $derived([...new Set([options.ssid, ...options.networks].filter((n): n is string => !!n))])
  const keepPowerMode = $derived(
    live.app?.matchPowerMode && draft.actions.thermal
      ? `Match thermal mode (${POWER_MODE_LABEL[powerModeFor(draft.actions.thermal)]})`
      : "Don't change",
  )
  const ifTitle = $derived(
    manual
      ? 'Started by hand'
      : draft.conditions.length > 1
        ? 'All of these'
        : CONDITION_KINDS.find((k) => k.kind === draft.conditions[0].kind)?.label,
  )

  function has(kind: ConditionKind): boolean {
    return draft.conditions.some((c) => c.kind === kind)
  }

  function toggle(kind: ConditionKind) {
    const next = has(kind)
      ? draft.conditions.filter((c) => c.kind !== kind)
      : [...draft.conditions, defaultCondition(kind, options.ssid)]
    draft.conditions = CONDITION_KINDS.flatMap((k) => next.filter((c) => c.kind === k.kind))
  }

  function pickedOther(listed: string[], value: string, other: boolean): string {
    return other || !listed.includes(value) ? OTHER : value
  }

  function save() {
    const r = $state.snapshot(draft) as Routine
    r.name = shownName.trim()
    for (const c of r.conditions) {
      if (c.kind === 'wifi') c.ssid = c.ssid.trim()
      if (c.kind === 'bluetooth') c.device = c.device.trim()
    }
    if (isInstant(r)) r.restore = false
    onsave(r)
  }

  const number = (e: Event) => Math.round(Number((e.currentTarget as HTMLInputElement).value)) || 0
</script>

<section class="editor" aria-label={isNew ? 'New routine' : 'Edit routine'}>
  <h2>{isNew ? 'New routine' : 'Edit routine'}</h2>

  <div class="block">
    <div class="title"><span class="badge if">If</span>{ifTitle}</div>
    <div class="tiles" role="group" aria-label="Conditions">
      <button
        type="button"
        class="tile"
        aria-pressed={manual}
        onclick={() => (draft.conditions = [])}
      >
        <Icon name="play" size={20} /><span>Manually</span>
      </button>
      {#each CONDITION_KINDS as k (k.kind)}
        <button type="button" class="tile" aria-pressed={has(k.kind)} onclick={() => toggle(k.kind)}>
          <Icon name={k.icon} size={20} /><span>{k.label}</span>
        </button>
      {/each}
    </div>

    {#each draft.conditions as c (c.kind)}
      <div class="condition">
        <span class="label">{CONDITION_LABEL[c.kind]}</span>
        {#if c.kind === 'wifi'}
          <ChipGroup
            label="Wi-Fi network"
            options={[...networks.map((n) => ({ value: n, label: n })), { value: OTHER, label: 'Other', icon: 'add' }]}
            value={pickedOther(networks, c.ssid, otherSsid)}
            onpick={(v) => {
              otherSsid = v === OTHER
              if (v !== OTHER) c.ssid = v
              else if (networks.includes(c.ssid)) c.ssid = ''
            }}
          />
          {#if pickedOther(networks, c.ssid, otherSsid) === OTHER}
            <input aria-label="Network name" bind:value={c.ssid} placeholder="Network name" />
          {/if}
          <ChipGroup
            label="Wi-Fi state"
            options={[
              { value: 'on', label: 'Connected' },
              { value: 'off', label: 'Not connected' },
            ]}
            value={c.connected ? 'on' : 'off'}
            onpick={(v) => (c.connected = v === 'on')}
          />
        {:else if c.kind === 'power'}
          <ChipGroup
            label="Power source"
            options={[
              { value: 'on', label: 'Plugged in', icon: 'plug' },
              { value: 'off', label: 'On battery', icon: 'battery' },
            ]}
            value={c.plugged ? 'on' : 'off'}
            onpick={(v) => (c.plugged = v === 'on')}
          />
        {:else if c.kind === 'time'}
          <ChipGroup
            label="Days"
            options={DAYS.map((d, i) => ({ value: String(i), label: d }))}
            value={c.days.map(String)}
            onpick={(v) => {
              const day = Number(v)
              c.days = c.days.includes(day) ? c.days.filter((d) => d !== day) : [...c.days, day].sort()
            }}
          />
          <div class="row">
            <ChipGroup
              label="When"
              options={[
                { value: 'at', label: 'At' },
                { value: 'between', label: 'Between' },
              ]}
              value={c.until === null ? 'at' : 'between'}
              onpick={(v) => (c.until = v === 'at' ? null : (c.until ?? (c.time === '17:00' ? '18:00' : '17:00')))}
            />
            <input
              type="time"
              aria-label="Time"
              value={c.time}
              onchange={(e) => {
                if (e.currentTarget.value) c.time = e.currentTarget.value
              }}
            />
            {#if c.until !== null}
              <span class="secondary">to</span>
              <input
                type="time"
                aria-label="Until"
                value={c.until}
                onchange={(e) => {
                  if (e.currentTarget.value) c.until = e.currentTarget.value
                }}
              />
            {/if}
          </div>
        {:else if c.kind === 'display'}
          <ChipGroup
            label="Display state"
            options={[
              { value: 'on', label: 'Connected' },
              { value: 'off', label: 'Not connected' },
            ]}
            value={c.connected ? 'on' : 'off'}
            onpick={(v) => (c.connected = v === 'on')}
          />
        {:else if c.kind === 'bluetooth'}
          <ChipGroup
            label="Bluetooth device"
            options={[
              ...options.bluetooth.map((n) => ({ value: n, label: n })),
              { value: OTHER, label: 'Other', icon: 'add' },
            ]}
            value={pickedOther(options.bluetooth, c.device, otherDevice)}
            onpick={(v) => {
              otherDevice = v === OTHER
              if (v !== OTHER) c.device = v
              else if (options.bluetooth.includes(c.device)) c.device = ''
            }}
          />
          {#if pickedOther(options.bluetooth, c.device, otherDevice) === OTHER}
            <input aria-label="Device name" bind:value={c.device} placeholder="Paired device" />
          {/if}
          <ChipGroup
            label="Bluetooth state"
            options={[
              { value: 'on', label: 'Connected' },
              { value: 'off', label: 'Not connected' },
            ]}
            value={c.connected ? 'on' : 'off'}
            onpick={(v) => (c.connected = v === 'on')}
          />
        {:else if c.kind === 'batteryBelow'}
          <div class="row">
            <ChipGroup
              label="Battery below"
              options={[...PERCENTS.map((p) => ({ value: String(p), label: `${p}%` })), { value: OTHER, label: 'Other' }]}
              value={pickedOther(PERCENTS.map(String), String(c.percent), otherPct)}
              onpick={(v) => {
                otherPct = v === OTHER
                if (v !== OTHER) c.percent = Number(v)
              }}
            />
            {#if pickedOther(PERCENTS.map(String), String(c.percent), otherPct) === OTHER}
              <input
                type="number"
                aria-label="Battery percent"
                min="5"
                max="95"
                value={c.percent}
                oninput={(e) => (c.percent = number(e))}
              />
            {/if}
          </div>
        {:else if c.kind === 'longAc'}
          <div class="row">
            <ChipGroup
              label="Plugged in for"
              options={[
                ...AC_DAYS.map((d) => ({ value: String(d), label: d === 1 ? '1 day' : `${d} days` })),
                { value: OTHER, label: 'Other' },
              ]}
              value={pickedOther(AC_DAYS.map(String), String(c.days), otherDays)}
              onpick={(v) => {
                otherDays = v === OTHER
                if (v !== OTHER) c.days = Number(v)
              }}
            />
            {#if pickedOther(AC_DAYS.map(String), String(c.days), otherDays) === OTHER}
              <input
                type="number"
                aria-label="Days count"
                min="1"
                max="60"
                value={c.days}
                oninput={(e) => (c.days = number(e))}
              />
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="block">
    <div class="title"><span class="badge then">Then</span>Change</div>
    <span class="label">Thermal mode</span>
    <ChipGroup
      label="Thermal mode"
      options={[{ value: KEEP, label: "Don't change" }, ...MODES.map((m) => ({ value: m, label: THERMAL_LABEL[m] }))]}
      value={draft.actions.thermal ?? KEEP}
      onpick={(v) => (draft.actions.thermal = (v || null) as ThermalMode | null)}
    />
    <span class="label">Battery profile</span>
    <ChipGroup
      label="Battery profile"
      options={[
        { value: KEEP, label: "Don't change" },
        ...PROFILES.map((p) => ({ value: p, label: PROFILE_LABEL[p] })),
      ]}
      value={draft.actions.battery ?? KEEP}
      onpick={(v) => (draft.actions.battery = (v || null) as BatteryProfile | null)}
    />
    <span class="label">Windows power mode</span>
    <ChipGroup
      label="Windows power mode"
      options={[
        { value: KEEP, label: keepPowerMode },
        ...POWER_MODES.map((m) => ({ value: m, label: POWER_MODE_SHORT[m] })),
      ]}
      value={draft.actions.powerMode ?? KEEP}
      onpick={(v) => (draft.actions.powerMode = (v || null) as PowerMode | null)}
    />
    <div class="checks">
      <label class="check">
        <input type="checkbox" checked={draft.notify} onchange={() => (draft.notify = !draft.notify)} />
        Notify
      </label>
      {#if !instant}
        <label class="check">
          <input type="checkbox" checked={draft.restore} onchange={() => (draft.restore = !draft.restore)} />
          {manual ? 'Undo when it stops' : 'Undo when it ends'}
        </label>
      {/if}
    </div>
  </div>

  {#if problem}<p class="hint">{problem}</p>{/if}

  <footer>
    <input
      aria-label="Name"
      class="name"
      maxlength={NAME_MAX}
      value={shownName}
      oninput={(e) => {
        named = true
        draft.name = e.currentTarget.value
      }}
    />
    <span class="spacer"></span>
    {#if !isNew}
      <button type="button" class="btn" onclick={ondelete}><Icon name="delete" /> Delete</button>
    {/if}
    <button type="button" class="btn" onclick={oncancel}>Cancel</button>
    <button type="button" class="btn accent" disabled={!!problem || !shownName.trim()} onclick={save}>Save</button>
  </footer>
</section>

<style>
  .editor {
    display: grid;
    gap: 12px;
    max-width: 880px;
    padding: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  .block {
    display: grid;
    gap: 12px;
    padding: 16px;
    background: var(--sunk);
    border-radius: var(--r-card);
  }

  .title {
    display: flex;
    align-items: center;
    gap: 12px;
    font-weight: 600;
  }

  .badge {
    padding: 2px 10px;
    font-size: 13px;
    font-weight: 600;
    border-radius: var(--r-ctl);
  }

  .badge.if {
    color: var(--accent);
    background: rgb(76 194 255 / 0.15);
  }

  .badge.then {
    color: #6ccb5f;
    background: rgb(108 203 95 / 0.15);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 8px;
  }

  .tile {
    display: grid;
    place-items: center;
    gap: 8px;
    min-height: 84px;
    padding: 12px 6px;
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
    cursor: default;
  }

  .tile:hover {
    background: var(--surface-hover);
  }

  .tile[aria-pressed='true'] {
    color: var(--accent);
    border-color: var(--accent);
    background: rgb(76 194 255 / 0.1);
  }

  .condition {
    display: grid;
    gap: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .label {
    font-size: 13px;
    color: var(--text-2);
  }

  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .spacer {
    flex: 1;
  }

  input:not([type='checkbox']) {
    height: 32px;
    padding: 0 10px;
    font: inherit;
    color: inherit;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
  }

  input:not([type]),
  input.name {
    width: 240px;
    max-width: 100%;
  }

  input[type='number'] {
    width: 72px;
  }

  .btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
    font: inherit;
    color: inherit;
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
</style>
