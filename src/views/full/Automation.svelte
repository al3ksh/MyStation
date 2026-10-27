<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Switch from '../../components/Switch.svelte'
  import { api } from '../../lib/api'
  import { POWER_MODE_LABEL, PROFILE_LABEL, THERMAL_LABEL, powerModeFor } from '../../lib/labels'
  import {
    DAYS,
    TRIGGER_KINDS,
    defaultTrigger,
    describeActions,
    describeTrigger,
    newId,
    triggerIcon,
    triggerProblem,
  } from '../../lib/routines'
  import { live } from '../../lib/telemetry.svelte'
  import { errorText, toasts } from '../../lib/toasts.svelte'
  import type {
    BatteryProfile,
    PowerMode,
    Routine,
    RoutineOptions,
    ThermalMode,
    TriggerKind,
  } from '../../lib/types'

  const PROFILES = Object.keys(PROFILE_LABEL) as BatteryProfile[]
  const MODES = Object.keys(THERMAL_LABEL) as ThermalMode[]
  const POWER_MODES = Object.keys(POWER_MODE_LABEL) as PowerMode[]
  const STEPS = ['When', 'Do', 'Name']
  const KEEP = ''

  let routines = $state<Routine[] | null>(null)
  let options = $state<RoutineOptions>({ ssid: null, networks: [], bluetooth: [] })

  /** The routine in the wizard; `index` is where it goes, `null` for a new one. */
  let draft = $state<Routine | null>(null)
  let index = $state<number | null>(null)
  let step = $state(0)
  let named = $state(false)

  onMount(() => {
    api.getRoutines().then(
      (r) => (routines = r),
      (e) => toasts.push(errorText(e)),
    )
  })

  // Every change saves at once, like Windows Settings; saves run in order.
  let saving = Promise.resolve()
  function save(next: Routine[]) {
    routines = next
    saving = saving.then(async () => {
      try {
        await api.setRoutines(next)
      } catch (e) {
        toasts.push(errorText(e))
        routines = await api.getRoutines().catch(() => routines)
      }
    })
  }

  function edit(change: (r: Routine[]) => void) {
    if (!routines) return
    const next = $state.snapshot(routines) as Routine[]
    change(next)
    save(next)
  }

  function move(i: number, by: number) {
    edit((r) => r.splice(i + by, 0, ...r.splice(i, 1)))
  }

  function loadOptions() {
    api.routineOptions().then(
      (o) => {
        options = o
        // A new Wi-Fi routine starts on the current network once it's known.
        if (draft?.trigger.kind === 'wifi' && !draft.trigger.ssid && o.ssid) draft.trigger.ssid = o.ssid
      },
      () => {},
    )
  }

  function open(i: number | null) {
    const r = i === null ? null : routines?.[i]
    draft = r
      ? ($state.snapshot(r) as Routine)
      : {
          id: newId(),
          name: '',
          enabled: true,
          trigger: defaultTrigger('wifi', options.ssid),
          actions: { thermal: null, battery: null, powerMode: null },
          notify: false,
        }
    index = r ? i : null
    named = !!r
    step = 0
    loadOptions()
  }

  function close() {
    draft = null
  }

  function setKind(kind: TriggerKind) {
    if (draft) draft.trigger = defaultTrigger(kind, options.ssid)
  }

  function toggleDay(day: number) {
    if (draft?.trigger.kind !== 'time') return
    const days = draft.trigger.days
    draft.trigger.days = days.includes(day) ? days.filter((d) => d !== day) : [...days, day].sort()
  }

  function next() {
    if (!draft) return
    step += 1
    if (step === 2 && !named) draft.name = describeTrigger(draft.trigger)
  }

  function finish() {
    if (!draft) return
    const r = $state.snapshot(draft) as Routine
    r.name = r.name.trim()
    edit((list) => {
      if (index === null) list.push(r)
      else list[index] = r
    })
    close()
  }

  const problem = $derived(draft ? triggerProblem(draft.trigger) : null)
  const doesNothing = $derived(
    !!draft && !draft.actions.thermal && !draft.actions.battery && !draft.actions.powerMode && !draft.notify,
  )
  const keepPowerMode = $derived(
    live.app?.matchPowerMode && draft?.actions.thermal
      ? `Match thermal mode (${POWER_MODE_LABEL[powerModeFor(draft.actions.thermal)]})`
      : "Don't change",
  )
</script>

{#if draft}
  {@const t = draft.trigger}
  <section class="wizard" aria-label={index === null ? 'New routine' : 'Edit routine'}>
    <header>
      <h2>{index === null ? 'New routine' : 'Edit routine'}</h2>
      <ol class="steps">
        {#each STEPS as s, i (s)}
          <li aria-current={i === step ? 'step' : undefined} class:done={i < step}>{s}</li>
        {/each}
      </ol>
    </header>

    <div class="body">
      {#if step === 0}
        <label class="sub">
          <span>When</span>
          <select aria-label="Trigger" value={t.kind} onchange={(e) => setKind(e.currentTarget.value as TriggerKind)}>
            {#each TRIGGER_KINDS as k (k.kind)}
              <option value={k.kind}>{k.label}</option>
            {/each}
          </select>
        </label>

        {#if t.kind === 'wifi'}
          <label class="sub">
            <span>Network</span>
            <input aria-label="Network" list="routine-networks" bind:value={t.ssid} placeholder="Network name" />
            <datalist id="routine-networks">
              {#each options.networks as n (n)}<option value={n}></option>{/each}
            </datalist>
          </label>
          <label class="sub">
            <span>Runs when the laptop</span>
            <select
              aria-label="Wi-Fi event"
              value={t.connected ? 'on' : 'off'}
              onchange={(e) => (t.connected = e.currentTarget.value === 'on')}
            >
              <option value="on">Connects to it</option>
              <option value="off">Disconnects from it</option>
            </select>
          </label>
        {:else if t.kind === 'power'}
          <label class="sub">
            <span>Runs when the laptop is</span>
            <select
              aria-label="Power source"
              value={t.plugged ? 'on' : 'off'}
              onchange={(e) => (t.plugged = e.currentTarget.value === 'on')}
            >
              <option value="on">Plugged in</option>
              <option value="off">Unplugged</option>
            </select>
          </label>
        {:else if t.kind === 'time'}
          <div class="sub">
            <span>Days</span>
            <div class="days">
              {#each DAYS as name, day (day)}
                <button type="button" class="day" aria-pressed={t.days.includes(day)} onclick={() => toggleDay(day)}
                  >{name}</button
                >
              {/each}
            </div>
          </div>
          <label class="sub">
            <span>Time</span>
            <input
              type="time"
              aria-label="Time"
              value={t.time}
              onchange={(e) => {
                if (e.currentTarget.value) t.time = e.currentTarget.value
              }}
            />
          </label>
        {:else if t.kind === 'display'}
          <label class="sub">
            <span>Runs when an external display is</span>
            <select
              aria-label="Display event"
              value={t.connected ? 'on' : 'off'}
              onchange={(e) => (t.connected = e.currentTarget.value === 'on')}
            >
              <option value="on">Connected</option>
              <option value="off">Disconnected</option>
            </select>
          </label>
        {:else if t.kind === 'bluetooth'}
          <label class="sub">
            <span>Device</span>
            <input aria-label="Device" list="routine-devices" bind:value={t.device} placeholder="Paired device" />
            <datalist id="routine-devices">
              {#each options.bluetooth as n (n)}<option value={n}></option>{/each}
            </datalist>
          </label>
          <label class="sub">
            <span>Runs when it</span>
            <select
              aria-label="Bluetooth event"
              value={t.connected ? 'on' : 'off'}
              onchange={(e) => (t.connected = e.currentTarget.value === 'on')}
            >
              <option value="on">Connects</option>
              <option value="off">Disconnects</option>
            </select>
          </label>
        {:else if t.kind === 'batteryBelow'}
          <label class="sub">
            <span>Battery drops below (%)</span>
            <input
              type="number"
              aria-label="Battery level"
              min="5"
              max="95"
              value={t.percent}
              oninput={(e) => (t.percent = Math.round(e.currentTarget.valueAsNumber) || 0)}
            />
          </label>
        {:else if t.kind === 'longAc'}
          <label class="sub">
            <span>Plugged in without a break for (days)</span>
            <input
              type="number"
              aria-label="Days on AC"
              min="1"
              max="60"
              value={t.days}
              oninput={(e) => (t.days = Math.round(e.currentTarget.valueAsNumber) || 0)}
            />
          </label>
        {/if}
        {#if problem}<p class="hint">{problem}</p>{/if}
      {:else if step === 1}
        <label class="sub">
          <span>Thermal mode</span>
          <select
            aria-label="Thermal mode"
            value={draft.actions.thermal ?? KEEP}
            onchange={(e) => draft && (draft.actions.thermal = (e.currentTarget.value || null) as ThermalMode | null)}
          >
            <option value={KEEP}>Don't change</option>
            {#each MODES as m (m)}<option value={m}>{THERMAL_LABEL[m]}</option>{/each}
          </select>
        </label>
        <label class="sub">
          <span>Battery profile</span>
          <select
            aria-label="Battery profile"
            value={draft.actions.battery ?? KEEP}
            onchange={(e) =>
              draft && (draft.actions.battery = (e.currentTarget.value || null) as BatteryProfile | null)}
          >
            <option value={KEEP}>Don't change</option>
            {#each PROFILES as p (p)}<option value={p}>{PROFILE_LABEL[p]}</option>{/each}
          </select>
        </label>
        <label class="sub">
          <span>Windows power mode</span>
          <select
            aria-label="Windows power mode"
            value={draft.actions.powerMode ?? KEEP}
            onchange={(e) => draft && (draft.actions.powerMode = (e.currentTarget.value || null) as PowerMode | null)}
          >
            <option value={KEEP}>{keepPowerMode}</option>
            {#each POWER_MODES as m (m)}<option value={m}>{POWER_MODE_LABEL[m]}</option>{/each}
          </select>
        </label>
        <div class="sub">
          <span>Show a notification</span>
          <Switch
            checked={draft.notify}
            label="Show a notification"
            onchange={() => draft && (draft.notify = !draft.notify)}
          />
        </div>
        {#if doesNothing}<p class="hint">Pick something to change, or turn on the notification</p>{/if}
      {:else}
        <label class="sub">
          <span>Name</span>
          <input aria-label="Name" class="name" bind:value={draft.name} oninput={() => (named = true)} />
        </label>
        <p class="secondary summary">
          {describeTrigger(t)} → {describeActions(draft.actions, draft.notify)}
        </p>
      {/if}
    </div>

    <footer>
      <button type="button" class="btn" onclick={close}>Cancel</button>
      <span class="spacer"></span>
      {#if step > 0}
        <button type="button" class="btn" onclick={() => (step -= 1)}>Back</button>
      {/if}
      {#if step < 2}
        <button
          type="button"
          class="btn accent"
          disabled={(step === 0 && !!problem) || (step === 1 && doesNothing)}
          onclick={next}>Next</button
        >
      {:else}
        <button type="button" class="btn accent" disabled={!draft.name.trim()} onclick={finish}>Save</button>
      {/if}
    </footer>
  </section>
{:else if routines}
  <div class="stack">
    <div class="head">
      <p class="secondary">
        Each routine runs when its trigger happens. A change you make yourself holds until the next trigger. When two
        routines switch the same setting, the higher one wins.
      </p>
      <button type="button" class="btn accent" onclick={() => open(null)}>
        <Icon name="add" /> New routine
      </button>
    </div>

    {#each routines as r, i (r.id)}
      <div class="row" class:off={!r.enabled} role="group" aria-label={r.name}>
        <Icon name={triggerIcon(r.trigger.kind)} size={20} />
        <div class="text">
          <span>{r.name}</span>
          <span class="secondary">{describeTrigger(r.trigger)} → {describeActions(r.actions, r.notify)}</span>
        </div>
        <div class="tools">
          <button
            type="button"
            class="icon-btn"
            aria-label="Move {r.name} up"
            disabled={i === 0}
            onclick={() => move(i, -1)}><Icon name="up" /></button
          >
          <button
            type="button"
            class="icon-btn"
            aria-label="Move {r.name} down"
            disabled={i === routines.length - 1}
            onclick={() => move(i, 1)}><Icon name="down" /></button
          >
          <button type="button" class="icon-btn" aria-label="Edit {r.name}" onclick={() => open(i)}
            ><Icon name="edit" /></button
          >
          <button
            type="button"
            class="icon-btn"
            aria-label="Delete {r.name}"
            onclick={() => edit((list) => list.splice(i, 1))}><Icon name="delete" /></button
          >
        </div>
        <Switch
          checked={r.enabled}
          label={r.name}
          onchange={() => edit((list) => (list[i].enabled = !list[i].enabled))}
        />
      </div>
    {:else}
      <p class="secondary empty">
        No routines yet. Make one to switch modes when you join a network, plug in, dock or at a set time.
      </p>
    {/each}
  </div>
{/if}

<style>
  .stack {
    display: grid;
    gap: 4px;
    max-width: 880px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 24px;
    margin-bottom: 8px;
  }

  .head p {
    margin: 0;
    font-size: 13px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 68px;
    padding: 12px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .row.off .text {
    opacity: 0.6;
  }

  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .text .secondary {
    font-size: 12px;
  }

  .tools {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }

  .wizard {
    display: grid;
    max-width: 880px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .wizard header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px 8px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  .steps {
    display: flex;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
    color: var(--text-2);
  }

  .steps li[aria-current='step'] {
    color: var(--accent);
    font-weight: 600;
  }

  .steps li.done {
    color: inherit;
  }

  .body {
    display: grid;
    gap: 8px;
    padding: 8px 20px 16px;
    min-height: 180px;
    align-content: start;
  }

  .wizard footer {
    display: flex;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
  }

  .spacer {
    flex: 1;
  }

  .sub {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }

  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }

  .summary {
    margin: 0;
    font-size: 13px;
  }

  .days {
    display: flex;
    gap: 2px;
  }

  .day {
    width: 40px;
    height: 30px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
    font-size: 12px;
    cursor: default;
  }

  .day[aria-pressed='true'] {
    background: var(--accent);
    border-color: transparent;
    color: var(--on-accent);
  }

  select,
  input {
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
  }

  input[type='number'] {
    width: 72px;
  }

  select option {
    background: var(--solid);
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    background: none;
    border: none;
    border-radius: var(--r-ctl);
    cursor: default;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .icon-btn:disabled {
    opacity: 0.35;
  }

  .btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
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

  .empty {
    margin: 0;
    padding: 24px 0;
    font-size: 13px;
  }
</style>
