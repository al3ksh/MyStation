import { describe, expect, test } from 'vitest'
import { groups, track, type Extremes } from '../lib/monitor'
import type { AppState, Telemetry } from '../lib/types'

const T: Telemetry = {
  tsMs: 0,
  battery: {
    percent: 82,
    state: 'discharging',
    powerW: -12.5,
    remainingMwh: 70000,
    fullMwh: 85000,
    designMwh: 97000,
    wearPct: 12.4,
    voltageV: 12.6,
    cycles: null,
  },
  cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null },
  gpu: { state: 'active', tempC: 60, loadPct: 3 },
  fans: [
    { name: 'CPU', rpm: 2400 },
    { name: 'GPU', rpm: 2100 },
  ],
  mem: { usedMb: 10240, totalMb: 32768 },
  dimmC: 45,
  skinC: 38,
  power: { source: 'battery', totalW: 12.5, cpuW: 4.2, gpuW: 1.1, restW: 7.2 },
}

const row = (gs: ReturnType<typeof groups>, group: string, label: string) =>
  gs.find((g) => g.name === group)!.rows.find((r) => r.label === label)!

describe('monitor groups', () => {
  test('reads every sensor into its group', () => {
    const gs = groups(T, null, 0)
    expect(gs.map((g) => g.name)).toEqual(['CPU', 'GPU', 'Fans', 'Memory', 'Chassis', 'Battery'])
    expect(row(gs, 'CPU', 'Temperature').value).toBe(54)
    expect(row(gs, 'CPU', 'Clock').show(1972)).toBe('1,972 MHz')
    expect(row(gs, 'CPU', 'Power').value).toBe(4.2)
    expect(row(gs, 'GPU', 'Load').value).toBe(3)
    expect(row(gs, 'Fans', 'GPU').value).toBe(2100)
    expect(row(gs, 'Memory', 'Used').show(10240)).toBe('10.0 GB')
    expect(row(gs, 'Battery', 'Discharge rate').value).toBe(12.5)
    expect(row(gs, 'Battery', 'Voltage').show(12.6)).toBe('12.60 V')
  })

  test('an asleep GPU has no temperature or load', () => {
    const gs = groups({ ...T, gpu: { state: 'asleep' } }, null, 0)
    expect(row(gs, 'GPU', 'Temperature').value).toBeNull()
    expect(row(gs, 'GPU', 'Load').value).toBeNull()
  })

  test('no discharge rate while charging', () => {
    const gs = groups({ ...T, battery: { ...T.battery!, state: 'charging', powerW: 30 } }, null, 0)
    expect(row(gs, 'Battery', 'Discharge rate').value).toBeNull()
  })

  test('falls back to the app state battery when telemetry has none', () => {
    const app = { battery: { ...T.battery!, percent: 55 }, onBatterySince: null } as unknown as AppState
    const gs = groups({ ...T, battery: null }, app, 0)
    expect(row(gs, 'Battery', 'Charge').value).toBe(55)
  })

  test('on battery counts the minutes since unplugging', () => {
    const app = { battery: null, onBatterySince: { sinceMs: 0, sincePct: 100 } } as unknown as AppState
    const gs = groups(T, app, 134 * 60_000)
    const r = row(gs, 'Battery', 'On battery')
    expect(r.value).toBe(134)
    expect(r.show(134)).toBe('2 h 14 min')
    expect(r.record).toBe('batteryStintMin')
  })

  test('records sit on the rows they belong to', () => {
    const gs = groups(T, null, 0)
    expect(row(gs, 'CPU', 'Temperature').record).toBe('cpuTempC')
    expect(row(gs, 'GPU', 'Temperature').record).toBe('gpuTempC')
    expect(row(gs, 'Battery', 'Discharge rate').record).toBe('batteryDrawW')
    expect(gs.find((g) => g.name === 'Fans')!.record).toBe('fanRpm')
  })
})

describe('track', () => {
  test('keeps the lowest and highest value of each row', () => {
    let e: Extremes = {}
    e = track(e, groups(T, null, 0))
    e = track(e, groups({ ...T, cpu: { ...T.cpu, tempC: 80 } }, null, 0))
    e = track(e, groups({ ...T, cpu: { ...T.cpu, tempC: 60 } }, null, 0))
    expect(e['CPU/Temperature']).toEqual({ min: 54, max: 80 })
  })

  test('skips missing readings', () => {
    let e: Extremes = {}
    e = track(e, groups({ ...T, gpu: { state: 'asleep' } }, null, 0))
    expect(e['GPU/Temperature']).toBeUndefined()
    e = track(e, groups(T, null, 0))
    expect(e['GPU/Temperature']).toEqual({ min: 60, max: 60 })
  })

  test('on battery keeps no min or max', () => {
    const app = { battery: null, onBatterySince: { sinceMs: 0, sincePct: null } } as unknown as AppState
    const e = track({}, groups(T, app, 60_000))
    expect(e['Battery/On battery']).toBeUndefined()
  })
})
