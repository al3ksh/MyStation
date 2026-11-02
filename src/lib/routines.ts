import type { IconName } from './icons'
import { PROFILE_LABEL, THERMAL_LABEL } from './labels'
import type { Actions, BatteryProfile, Condition, ConditionKind, PowerMode, Routine, ThermalMode } from './types'

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

/** The editor's tiles, in the order conditions are kept. */
export const CONDITION_KINDS: { kind: ConditionKind; label: string; icon: IconName }[] = [
  { kind: 'wifi', label: 'Wi-Fi', icon: 'wifi' },
  { kind: 'power', label: 'Power', icon: 'plug' },
  { kind: 'time', label: 'Time', icon: 'calendar' },
  { kind: 'display', label: 'Display', icon: 'display' },
  { kind: 'bluetooth', label: 'Bluetooth', icon: 'bluetooth' },
  { kind: 'batteryBelow', label: 'Battery', icon: 'battery' },
  { kind: 'longAc', label: 'Days on AC', icon: 'batteryCharging' },
]

export function conditionIcon(kind: ConditionKind): IconName {
  return CONDITION_KINDS.find((k) => k.kind === kind)?.icon ?? 'automation'
}

export function describeDays(days: number[]): string {
  const sorted = [...days].sort()
  const key = sorted.join()
  if (key === '0,1,2,3,4') return 'Weekdays'
  if (key === '5,6') return 'Weekends'
  if (key === '0,1,2,3,4,5,6') return 'Every day'
  return sorted.map((d) => DAYS[d]).join(', ')
}

/** Short text for a card's chip. */
export function conditionChip(c: Condition): string {
  switch (c.kind) {
    case 'wifi':
      return c.connected ? c.ssid : `Not ${c.ssid}`
    case 'power':
      return c.plugged ? 'Plugged in' : 'On battery'
    case 'time':
      return `${describeDays(c.days)} ${c.time}${c.until ? `–${c.until}` : ''}`
    case 'display':
      return c.connected ? 'Display' : 'No display'
    case 'bluetooth':
      return c.connected ? c.device : `Not ${c.device}`
    case 'batteryBelow':
      return `Below ${c.percent}%`
    case 'longAc':
      return c.days === 1 ? '1 day on AC' : `${c.days} days on AC`
  }
}

/** What a new routine is called until its name is typed in. */
export function defaultName(r: Routine): string {
  return r.conditions.length ? r.conditions.map(conditionChip).join(', ') : 'Manual routine'
}

const THERMAL_ICON: Record<ThermalMode, IconName> = {
  Optimized: 'optimized',
  Cool: 'cool',
  Quiet: 'quiet',
  UltraPerformance: 'lightning',
}

const PROFILE_ICON: Record<BatteryProfile, IconName> = { home: 'home', campus: 'campus', storage: 'storage' }

/** The Windows power modes, short enough for a chip. */
export const POWER_MODE_SHORT: Record<PowerMode, string> = {
  BestEfficiency: 'Efficiency',
  Balanced: 'Balanced',
  BestPerformance: 'Performance',
}

export function actionChips(a: Actions): { icon: IconName; text: string }[] {
  const chips: { icon: IconName; text: string }[] = []
  if (a.thermal) chips.push({ icon: THERMAL_ICON[a.thermal], text: THERMAL_LABEL[a.thermal] })
  if (a.battery) chips.push({ icon: PROFILE_ICON[a.battery], text: PROFILE_LABEL[a.battery] })
  if (a.powerMode) chips.push({ icon: 'gauge', text: POWER_MODE_SHORT[a.powerMode] })
  return chips
}

export function defaultCondition(kind: ConditionKind, ssid: string | null): Condition {
  switch (kind) {
    case 'power':
      return { kind, plugged: true }
    case 'time':
      return { kind, days: [0, 1, 2, 3, 4], time: '08:00', until: null }
    case 'wifi':
      return { kind, ssid: ssid ?? '', connected: true }
    case 'display':
      return { kind, connected: true }
    case 'bluetooth':
      return { kind, device: '', connected: true }
    case 'batteryBelow':
      return { kind, percent: 20 }
    case 'longAc':
      return { kind, days: 3 }
  }
}

/** What the backend would reject about a condition, or `null`. */
export function conditionProblem(c: Condition): string | null {
  switch (c.kind) {
    case 'wifi':
      return c.ssid.trim() ? null : 'Enter the Wi-Fi network'
    case 'bluetooth':
      return c.device.trim() ? null : 'Pick the Bluetooth device'
    case 'time':
      if (!c.days.length) return 'Pick at least one day'
      return c.until === c.time ? 'Pick an end time other than the start' : null
    case 'batteryBelow':
      return c.percent >= 5 && c.percent <= 95 ? null : 'Pick a level from 5% to 95%'
    case 'longAc':
      return c.days >= 1 && c.days <= 60 ? null : 'Pick from 1 to 60 days'
    default:
      return null
  }
}

/** The first thing that keeps a routine from being saved, or `null`. */
export function routineProblem(r: Routine): string | null {
  for (const c of r.conditions) {
    const problem = conditionProblem(c)
    if (problem) return problem
  }
  const changes = r.actions.thermal || r.actions.battery || r.actions.powerMode
  return changes || r.notify ? null : 'Pick something to change, or turn on Notify'
}

/** Started and stopped by hand. */
export function isManual(r: Routine): boolean {
  return r.conditions.length === 0
}

/** Runs at an "At" time and never over time. */
export function isInstant(r: Routine): boolean {
  return r.conditions.some((c) => c.kind === 'time' && !c.until)
}

export function newId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`
}
