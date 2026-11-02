import { describe, expect, test } from 'vitest'
import {
  actionChips,
  conditionChip,
  defaultCondition,
  defaultName,
  describeDays,
  isInstant,
  isManual,
  routineProblem,
} from '../lib/routines'
import type { Routine } from '../lib/types'

function routine(over: Partial<Routine> = {}): Routine {
  return {
    id: 'a',
    name: 'A',
    enabled: true,
    conditions: [{ kind: 'power', plugged: true }],
    actions: { thermal: 'Quiet', battery: null, powerMode: null },
    notify: false,
    restore: false,
    ...over,
  }
}

describe('conditionChip', () => {
  test('names each condition briefly', () => {
    expect(conditionChip({ kind: 'wifi', ssid: 'Office', connected: true })).toBe('Office')
    expect(conditionChip({ kind: 'wifi', ssid: 'Office', connected: false })).toBe('Not Office')
    expect(conditionChip({ kind: 'power', plugged: true })).toBe('Plugged in')
    expect(conditionChip({ kind: 'power', plugged: false })).toBe('On battery')
    expect(conditionChip({ kind: 'time', days: [0, 1, 2, 3, 4], time: '08:00', until: null })).toBe('Weekdays 08:00')
    expect(conditionChip({ kind: 'time', days: [0, 1, 2, 3, 4], time: '09:00', until: '17:00' })).toBe(
      'Weekdays 09:00–17:00',
    )
    expect(conditionChip({ kind: 'display', connected: true })).toBe('Display')
    expect(conditionChip({ kind: 'display', connected: false })).toBe('No display')
    expect(conditionChip({ kind: 'bluetooth', device: 'MX Master', connected: true })).toBe('MX Master')
    expect(conditionChip({ kind: 'bluetooth', device: 'MX Master', connected: false })).toBe('Not MX Master')
    expect(conditionChip({ kind: 'batteryBelow', percent: 20 })).toBe('Below 20%')
    expect(conditionChip({ kind: 'longAc', days: 1 })).toBe('1 day on AC')
    expect(conditionChip({ kind: 'longAc', days: 3 })).toBe('3 days on AC')
  })
})

describe('describeDays', () => {
  test('names common day sets', () => {
    expect(describeDays([0, 1, 2, 3, 4])).toBe('Weekdays')
    expect(describeDays([6, 5])).toBe('Weekends')
    expect(describeDays([0, 1, 2, 3, 4, 5, 6])).toBe('Every day')
    expect(describeDays([2, 0])).toBe('Mon, Wed')
  })
})

describe('defaultName', () => {
  test('joins the conditions', () => {
    const r = routine({
      conditions: [
        { kind: 'wifi', ssid: 'Office', connected: true },
        { kind: 'power', plugged: true },
      ],
    })
    expect(defaultName(r)).toBe('Office, Plugged in')
  })

  test('stays within the 60 characters a name may have', () => {
    const r = routine({
      conditions: [
        { kind: 'wifi', ssid: 'OfficeNet', connected: true },
        { kind: 'power', plugged: true },
        { kind: 'time', days: [0, 1, 2, 3, 4], time: '09:00', until: '17:00' },
        { kind: 'display', connected: true },
        { kind: 'bluetooth', device: 'WH-1000XM5', connected: true },
      ],
    })
    const name = defaultName(r)
    expect(name.length).toBeLessThanOrEqual(60)
    expect(name.startsWith('OfficeNet, Plugged in, Weekdays 09:00–17:00')).toBe(true)
    expect(name.endsWith('…')).toBe(true)
  })

  test('a routine without conditions is manual', () => {
    expect(defaultName(routine({ conditions: [] }))).toBe('Manual routine')
  })
})

describe('actionChips', () => {
  test('lists what the routine switches', () => {
    const chips = actionChips({ thermal: 'Quiet', battery: 'home', powerMode: 'BestEfficiency' })
    expect(chips.map((c) => c.text)).toEqual(['Quiet', 'Desk', 'Efficiency'])
    expect(chips.map((c) => c.icon)).toEqual(['quiet', 'home', 'gauge'])
    expect(actionChips({ thermal: null, battery: null, powerMode: null })).toEqual([])
  })
})

describe('routineProblem', () => {
  test('says what the backend would reject', () => {
    const time = (days: number[], until: string | null) =>
      routine({ conditions: [{ kind: 'time', days, time: '08:00', until }] })
    expect(routineProblem(time([0], '08:00'))).toBe('Pick an end time other than the start')
    expect(routineProblem(time([], null))).toBe('Pick at least one day')
    expect(routineProblem(time([0], '17:00'))).toBeNull()
    expect(routineProblem(routine({ conditions: [{ kind: 'wifi', ssid: '  ', connected: true }] }))).toBe(
      'Enter the Wi-Fi network',
    )
    expect(routineProblem(routine({ conditions: [{ kind: 'bluetooth', device: ' ', connected: true }] }))).toBe(
      'Pick the Bluetooth device',
    )
    expect(routineProblem(routine({ conditions: [{ kind: 'batteryBelow', percent: 2 }] }))).toBe(
      'Pick a level from 5% to 95%',
    )
    expect(routineProblem(routine({ conditions: [{ kind: 'longAc', days: 0 }] }))).toBe('Pick from 1 to 60 days')
    expect(routineProblem(routine({ actions: { thermal: null, battery: null, powerMode: null } }))).toBe(
      'Pick something to change, or turn on Notify',
    )
    expect(
      routineProblem(routine({ actions: { thermal: null, battery: null, powerMode: null }, notify: true })),
    ).toBeNull()
    expect(routineProblem(routine())).toBeNull()
  })
})

describe('kinds of routine', () => {
  test('no conditions is manual; an At time is instant', () => {
    expect(isManual(routine({ conditions: [] }))).toBe(true)
    expect(isManual(routine())).toBe(false)
    const at = routine({ conditions: [{ kind: 'time', days: [0], time: '08:00', until: null }] })
    const between = routine({ conditions: [{ kind: 'time', days: [0], time: '08:00', until: '09:00' }] })
    expect(isInstant(at)).toBe(true)
    expect(isInstant(between)).toBe(false)
    expect(isInstant(routine())).toBe(false)
  })
})

describe('defaultCondition', () => {
  test('starts each kind somewhere sensible', () => {
    expect(defaultCondition('time', null)).toEqual({ kind: 'time', days: [0, 1, 2, 3, 4], time: '08:00', until: null })
    expect(defaultCondition('wifi', 'TOTO4')).toEqual({ kind: 'wifi', ssid: 'TOTO4', connected: true })
    expect(defaultCondition('wifi', null)).toEqual({ kind: 'wifi', ssid: '', connected: true })
    expect(defaultCondition('batteryBelow', null)).toEqual({ kind: 'batteryBelow', percent: 20 })
  })
})
