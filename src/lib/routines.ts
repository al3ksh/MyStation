import type { IconName } from './icons'
import { POWER_MODE_LABEL, PROFILE_LABEL, THERMAL_LABEL } from './labels'
import type { Actions, Trigger, TriggerKind } from './types'

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

export const TRIGGER_KINDS: { kind: TriggerKind; label: string; icon: IconName }[] = [
  { kind: 'wifi', label: 'Wi-Fi network', icon: 'wifi' },
  { kind: 'power', label: 'Power source', icon: 'plug' },
  { kind: 'time', label: 'Time of day', icon: 'calendar' },
  { kind: 'display', label: 'External display', icon: 'display' },
  { kind: 'bluetooth', label: 'Bluetooth device', icon: 'bluetooth' },
  { kind: 'batteryBelow', label: 'Battery level', icon: 'battery' },
  { kind: 'longAc', label: 'Plugged in for days', icon: 'batteryCharging' },
]

export function triggerIcon(kind: TriggerKind): IconName {
  return TRIGGER_KINDS.find((k) => k.kind === kind)?.icon ?? 'automation'
}

function describeDays(days: number[]): string {
  const key = [...days].sort().join()
  if (key === '0,1,2,3,4') return 'Weekdays'
  if (key === '5,6') return 'Weekends'
  if (key === '0,1,2,3,4,5,6') return 'Every day'
  return [...days]
    .sort()
    .map((d) => DAYS[d])
    .join(', ')
}

/** Mirrors `Trigger::describe` in the core, except time names its days. */
export function describeTrigger(t: Trigger): string {
  switch (t.kind) {
    case 'power':
      return t.plugged ? 'Plugged in' : 'On battery'
    case 'time':
      return `${describeDays(t.days)} at ${t.time}`
    case 'wifi':
      return t.connected ? `Connected to ${t.ssid}` : `Disconnected from ${t.ssid}`
    case 'display':
      return t.connected ? 'External display connected' : 'External display disconnected'
    case 'bluetooth':
      return `${t.device} ${t.connected ? 'connected' : 'disconnected'}`
    case 'batteryBelow':
      return `Battery below ${t.percent}%`
    case 'longAc':
      return t.days === 1 ? 'Plugged in for a day' : `Plugged in for ${t.days} days`
  }
}

export function describeActions(a: Actions, notify: boolean): string {
  const parts: string[] = []
  if (a.thermal) parts.push(THERMAL_LABEL[a.thermal])
  if (a.battery) parts.push(`${PROFILE_LABEL[a.battery]} profile`)
  if (a.powerMode) parts.push(`Windows: ${POWER_MODE_LABEL[a.powerMode]}`)
  if (notify) parts.push('Notifies')
  return parts.join(' · ')
}

export function defaultTrigger(kind: TriggerKind, ssid: string | null): Trigger {
  switch (kind) {
    case 'power':
      return { kind, plugged: true }
    case 'time':
      return { kind, days: [0, 1, 2, 3, 4], time: '08:00' }
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

/** What the backend would reject about a trigger, or `null`. */
export function triggerProblem(t: Trigger): string | null {
  switch (t.kind) {
    case 'wifi':
      return t.ssid.trim() ? null : 'Enter the Wi-Fi network'
    case 'bluetooth':
      return t.device.trim() ? null : 'Pick the Bluetooth device'
    case 'time':
      return t.days.length ? null : 'Pick at least one day'
    case 'batteryBelow':
      return t.percent >= 5 && t.percent <= 95 ? null : 'Pick a level from 5% to 95%'
    case 'longAc':
      return t.days >= 1 && t.days <= 60 ? null : 'Pick from 1 to 60 days'
    default:
      return null
  }
}

export function newId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`
}
