import { describe, expect, test } from 'vitest'
import { defaultTrigger, describeActions, describeTrigger, triggerProblem } from '../lib/routines'

describe('describeTrigger', () => {
  test('names each trigger the way the notifications do', () => {
    expect(describeTrigger({ kind: 'power', plugged: true })).toBe('Plugged in')
    expect(describeTrigger({ kind: 'power', plugged: false })).toBe('On battery')
    expect(describeTrigger({ kind: 'wifi', ssid: 'Home', connected: true })).toBe('Connected to Home')
    expect(describeTrigger({ kind: 'wifi', ssid: 'Home', connected: false })).toBe('Disconnected from Home')
    expect(describeTrigger({ kind: 'display', connected: true })).toBe('External display connected')
    expect(describeTrigger({ kind: 'bluetooth', device: 'MX Master', connected: false })).toBe(
      'MX Master disconnected',
    )
    expect(describeTrigger({ kind: 'batteryBelow', percent: 20 })).toBe('Battery below 20%')
    expect(describeTrigger({ kind: 'longAc', days: 1 })).toBe('Plugged in for a day')
    expect(describeTrigger({ kind: 'longAc', days: 3 })).toBe('Plugged in for 3 days')
  })

  test('time triggers name common day sets', () => {
    expect(describeTrigger({ kind: 'time', days: [0, 1, 2, 3, 4], time: '08:00' })).toBe('Weekdays at 08:00')
    expect(describeTrigger({ kind: 'time', days: [5, 6], time: '10:00' })).toBe('Weekends at 10:00')
    expect(describeTrigger({ kind: 'time', days: [0, 1, 2, 3, 4, 5, 6], time: '22:30' })).toBe('Every day at 22:30')
    expect(describeTrigger({ kind: 'time', days: [0, 2], time: '07:00' })).toBe('Mon, Wed at 07:00')
  })
})

describe('describeActions', () => {
  test('lists what the routine switches', () => {
    expect(
      describeActions({ thermal: 'UltraPerformance', battery: 'home', powerMode: 'BestPerformance' }, true),
    ).toBe('Ultra Performance · Desk profile · Windows: Best performance · Notifies')
    expect(describeActions({ thermal: null, battery: null, powerMode: null }, true)).toBe('Notifies')
    expect(describeActions({ thermal: 'Quiet', battery: null, powerMode: null }, false)).toBe('Quiet')
    expect(describeActions({ thermal: 'Quiet', battery: null, powerMode: null }, true, true)).toBe(
      'Quiet · Restores · Notifies',
    )
  })
})

describe('defaultTrigger', () => {
  test('a Wi-Fi trigger starts on the current network', () => {
    expect(defaultTrigger('wifi', 'TOTO4')).toEqual({ kind: 'wifi', ssid: 'TOTO4', connected: true })
    expect(defaultTrigger('wifi', null)).toEqual({ kind: 'wifi', ssid: '', connected: true })
  })

  test('a time trigger starts on weekdays at 08:00', () => {
    expect(defaultTrigger('time', null)).toEqual({ kind: 'time', days: [0, 1, 2, 3, 4], time: '08:00' })
  })
})

describe('triggerProblem', () => {
  test('flags what the backend would reject', () => {
    expect(triggerProblem({ kind: 'wifi', ssid: '  ', connected: true })).toBe('Enter the Wi-Fi network')
    expect(triggerProblem({ kind: 'bluetooth', device: '', connected: true })).toBe('Pick the Bluetooth device')
    expect(triggerProblem({ kind: 'time', days: [], time: '08:00' })).toBe('Pick at least one day')
    expect(triggerProblem({ kind: 'batteryBelow', percent: 99 })).toBe('Pick a level from 5% to 95%')
    expect(triggerProblem({ kind: 'longAc', days: 0 })).toBe('Pick from 1 to 60 days')
    expect(triggerProblem({ kind: 'power', plugged: true })).toBeNull()
  })
})
