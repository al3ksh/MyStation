<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Switch from '../../components/Switch.svelte'
  import { api } from '../../lib/api'
  import { PROFILE_LABEL, THERMAL_LABEL } from '../../lib/labels'
  import { errorText, toasts } from '../../lib/toasts.svelte'
  import type { Automation, BatteryProfile, ScheduleEntry, ThermalMode } from '../../lib/types'

  const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
  const PROFILES = Object.keys(PROFILE_LABEL) as BatteryProfile[]
  const MODES = Object.keys(THERMAL_LABEL) as ThermalMode[]
  const KEEP = ''

  let rules = $state<Automation | null>(null)

  onMount(() => {
    api.getAutomation().then(
      (r) => (rules = r),
      (e) => toasts.push(errorText(e)),
    )
  })

  // Every change saves at once, like Windows Settings; saves run in order.
  let saving = Promise.resolve()
  function save(next: Automation) {
    rules = next
    saving = saving.then(async () => {
      try {
        await api.setAutomation(next)
      } catch (e) {
        toasts.push(errorText(e))
        rules = await api.getAutomation().catch(() => rules)
      }
    })
  }

  function edit(change: (r: Automation) => void) {
    if (!rules) return
    const next = $state.snapshot(rules) as Automation
    change(next)
    save(next)
  }

  function setEntry(i: number, change: (e: ScheduleEntry) => void) {
    edit((r) => change(r.schedule.entries[i]))
  }

  function toggleDay(i: number, day: number) {
    setEntry(i, (e) => {
      e.days = e.days.includes(day) ? e.days.filter((d) => d !== day) : [...e.days, day].sort()
    })
  }
</script>

{#snippet modeSelect(label: string, value: ThermalMode | null, set: (m: ThermalMode | null) => void)}
  <label class="sub">
    <span>{label}</span>
    <select
      value={value ?? KEEP}
      onchange={(e) => set((e.currentTarget.value || null) as ThermalMode | null)}
    >
      <option value={KEEP}>Don't change</option>
      {#each MODES as m (m)}
        <option value={m}>{THERMAL_LABEL[m]}</option>
      {/each}
    </select>
  </label>
{/snippet}

{#snippet profileSelect(label: string, value: BatteryProfile, set: (p: BatteryProfile) => void)}
  <select aria-label={label} {value} onchange={(e) => set(e.currentTarget.value as BatteryProfile)}>
    {#each PROFILES as p (p)}
      <option value={p}>{PROFILE_LABEL[p]}</option>
    {/each}
  </select>
{/snippet}

{#if rules}
  <div class="stack">
    <section>
      <div class="row" class:open={rules.thermal.enabled}>
        <Icon name="plug" size={20} />
        <div class="text">
          <span>Thermal mode by power source</span>
          <span class="secondary">Switch the thermal mode when you plug in or unplug</span>
        </div>
        <Switch
          checked={rules.thermal.enabled}
          label="Thermal mode by power source"
          onchange={() => edit((r) => (r.thermal.enabled = !r.thermal.enabled))}
        />
      </div>
      {#if rules.thermal.enabled}
        <div class="group">
          {@render modeSelect('Plugged in', rules.thermal.onAc, (m) => edit((r) => (r.thermal.onAc = m)))}
          {@render modeSelect('On battery', rules.thermal.onBattery, (m) => edit((r) => (r.thermal.onBattery = m)))}
        </div>
      {/if}
    </section>

    <section>
      <div class="row" class:open={rules.schedule.enabled}>
        <Icon name="calendar" size={20} />
        <div class="text">
          <span>Battery profile schedule</span>
          <span class="secondary">Each entry switches the profile at its time; it stays until the next change</span>
        </div>
        <Switch
          checked={rules.schedule.enabled}
          label="Battery profile schedule"
          onchange={() => edit((r) => (r.schedule.enabled = !r.schedule.enabled))}
        />
      </div>
      {#if rules.schedule.enabled}
        <div class="group">
          {#each rules.schedule.entries as entry, i (i)}
            <div class="entry" role="group" aria-label="Entry {i + 1}">
              <div class="days">
                {#each DAYS as name, day (day)}
                  {@const on = entry.days.includes(day)}
                  <button
                    type="button"
                    class="day"
                    aria-pressed={on}
                    disabled={on && entry.days.length === 1}
                    onclick={() => toggleDay(i, day)}>{name}</button
                  >
                {/each}
              </div>
              <input
                type="time"
                aria-label="Time"
                value={entry.time}
                onchange={(e) => {
                  const time = e.currentTarget.value
                  if (time) setEntry(i, (en) => (en.time = time))
                }}
              />
              {@render profileSelect('Profile', entry.profile, (p) => setEntry(i, (en) => (en.profile = p)))}
              <button
                type="button"
                class="icon-btn"
                aria-label="Remove entry {i + 1}"
                onclick={() => edit((r) => r.schedule.entries.splice(i, 1))}
              >
                <Icon name="delete" />
              </button>
            </div>
          {:else}
            <p class="secondary empty">No entries yet.</p>
          {/each}
          <button
            type="button"
            class="btn"
            onclick={() =>
              edit((r) => r.schedule.entries.push({ days: [0, 1, 2, 3, 4], time: '08:00', profile: 'campus' }))}
          >
            <Icon name="add" /> Add entry
          </button>
        </div>
      {/if}
    </section>

    <section>
      <div class="row" class:open={rules.longAc.enabled}>
        <Icon name="battery" size={20} />
        <div class="text">
          <span>Long time on AC</span>
          <span class="secondary">Protect the battery when the laptop stays plugged in; back to the schedule when you unplug</span>
        </div>
        <Switch
          checked={rules.longAc.enabled}
          label="Long time on AC"
          onchange={() => edit((r) => (r.longAc.enabled = !r.longAc.enabled))}
        />
      </div>
      {#if rules.longAc.enabled}
        <div class="group">
          <label class="sub">
            <span>After days on AC</span>
            <input
              type="number"
              min="1"
              max="60"
              value={rules.longAc.days}
              onchange={(e) => {
                const days = Math.round(e.currentTarget.valueAsNumber)
                if (days >= 1 && days <= 60) edit((r) => (r.longAc.days = days))
                else e.currentTarget.value = String(rules?.longAc.days)
              }}
            />
          </label>
          <div class="sub">
            <span>Switch to</span>
            {@render profileSelect('Long AC profile', rules.longAc.profile, (p) => edit((r) => (r.longAc.profile = p)))}
          </div>
        </div>
      {/if}
    </section>

    <section>
      <div class="row">
        <Icon name="bell" size={20} />
        <div class="text">
          <span>Notifications</span>
          <span class="secondary">Show a notification for every automatic change</span>
        </div>
        <Switch
          checked={rules.notify}
          label="Notifications"
          onchange={() => edit((r) => (r.notify = !r.notify))}
        />
      </div>
    </section>
  </div>
{/if}

<style>
  .stack {
    display: grid;
    gap: 12px;
    max-width: 880px;
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

  .row.open {
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
  }

  .text {
    display: grid;
    gap: 2px;
  }

  .text .secondary {
    font-size: 12px;
  }

  .group {
    display: grid;
    gap: 8px;
    padding: 12px 16px 12px 52px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: none;
    border-radius: 0 0 var(--r-card) var(--r-card);
  }

  .sub {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .days {
    display: flex;
    gap: 2px;
    margin-right: auto;
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

  .day:disabled {
    opacity: 1;
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

  .icon-btn:hover {
    background: var(--surface-hover);
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    justify-self: start;
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

  .empty {
    margin: 0;
    font-size: 13px;
  }
</style>
