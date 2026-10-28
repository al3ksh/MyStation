import { describe, expect, test } from 'vitest'
import { groups, isHot, temperatures, track, type Extremes } from '../lib/monitor'
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
    cycles: 41,
  },
  cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null, coreLoads: [12.5, 3] },
  gpu: { state: 'active', tempC: 60, loadPct: 3, clockMhz: 1395, memClockMhz: 5501, vramUsedMb: 512, vramTotalMb: 4096 },
  fans: [
    { name: 'CPU', rpm: 2400 },
    { name: 'GPU', rpm: 2100 },
  ],
  mem: { usedMb: 10240, totalMb: 32768 },
  dimmC: 45,
  skinC: 38,
  drives: [{ index: 0, model: 'Samsung PM9A1', tempC: 41, activePct: 7.5, readBps: 1_500_000, writeBps: 20_480 }],
  power: { source: 'battery', totalW: 12.5, cpuW: 4.2, gpuW: 1.1, restW: 7.2 },
}

const row = (gs: ReturnType<typeof groups>, group: string, label: string) =>
  gs.find((g) => g.name === group)!.rows.find((r) => r.label === label)!

describe('monitor groups', () => {
  test('reads every sensor into its group', () => {
    const gs = groups(T, null, 0)
    expect(gs.map((g) => g.name)).toEqual(['CPU', 'CPU threads', 'GPU', 'Fans', 'Memory', 'Chassis', 'Samsung PM9A1', 'Battery'])
    expect(row(gs, 'CPU', 'Temperature').value).toBe(54)
    expect(row(gs, 'CPU', 'Clock').show(1972)).toBe('1,972 MHz')
    expect(row(gs, 'CPU', 'Power').value).toBe(4.2)
    expect(row(gs, 'GPU', 'Load').value).toBe(3)
    expect(row(gs, 'GPU', 'Memory clock').value).toBe(5501)
    expect(row(gs, 'GPU', 'Memory used').show(512)).toBe('0.5 GB')
    expect(row(gs, 'Fans', 'GPU').value).toBe(2100)
    expect(row(gs, 'Memory', 'Used').show(10240)).toBe('10.0 GB')
    expect(row(gs, 'Memory', 'Load').value).toBe(31.25)
    expect(row(gs, 'Battery', 'Discharge rate').value).toBe(12.5)
    expect(row(gs, 'Battery', 'Voltage').show(12.6)).toBe('12.60 V')
    expect(row(gs, 'Battery', 'Wear level').show(12.4)).toBe('12.4%')
    expect(row(gs, 'Battery', 'Design capacity').show(97000)).toBe('97.0 Wh')
    expect(row(gs, 'Battery', 'Cycles').value).toBe(41)
  })

  test('every logical processor has its own load row', () => {
    const gs = groups(T, null, 0)
    expect(row(gs, 'CPU threads', 'CPU 0').value).toBe(12.5)
    expect(row(gs, 'CPU threads', 'CPU 1').value).toBe(3)
  })

  test('each drive is a group with its temperature and activity', () => {
    const gs = groups(T, null, 0)
    const g = gs.find((g) => g.name === 'Samsung PM9A1')!
    expect(g.icon).toBe('drive')
    expect(row(gs, 'Samsung PM9A1', 'Temperature').value).toBe(41)
    expect(row(gs, 'Samsung PM9A1', 'Activity').value).toBe(7.5)
    expect(row(gs, 'Samsung PM9A1', 'Read rate').show(1_500_000)).toBe('1.4 MB/s')
    expect(row(gs, 'Samsung PM9A1', 'Write rate').show(20_480)).toBe('20 KB/s')
  })

  test('drives without a model or with the same one keep apart', () => {
    const d = T.drives[0]
    const gs = groups({ ...T, drives: [d, { ...d, index: 1 }, { ...d, index: 2, model: null }] }, null, 0)
    expect(gs.map((g) => g.name).slice(6, 9)).toEqual(['Samsung PM9A1 (Disk 0)', 'Samsung PM9A1 (Disk 1)', 'Disk 2'])
  })

  test('each group has an icon', () => {
    expect(groups(T, null, 0).map((g) => g.icon)).toEqual(['cpu', 'cpu', 'gpu', 'fan', 'memory', 'laptop', 'drive', 'battery'])
  })

  test('temperatures run hot past the limit of their part', () => {
    const gs = groups(T, null, 0)
    expect(isHot(row(gs, 'CPU', 'Temperature'), 89)).toBe(false)
    expect(isHot(row(gs, 'CPU', 'Temperature'), 90)).toBe(true)
    expect(isHot(row(gs, 'GPU', 'Temperature'), 85)).toBe(true)
    expect(isHot(row(gs, 'Samsung PM9A1', 'Temperature'), 70)).toBe(true)
    expect(isHot(row(gs, 'Chassis', 'Temperature'), 49)).toBe(false)
    expect(isHot(row(gs, 'CPU', 'Load'), 100)).toBe(false)
  })

  test('an asleep GPU has no temperature or load', () => {
    const gs = groups({ ...T, gpu: { state: 'asleep' } }, null, 0)
    expect(row(gs, 'GPU', 'Temperature').value).toBeNull()
    expect(row(gs, 'GPU', 'Load').value).toBeNull()
    expect(row(gs, 'GPU', 'Core clock').value).toBeNull()
  })

  test('no discharge rate while charging', () => {
    const gs = groups({ ...T, battery: { ...T.battery!, state: 'charging', powerW: 30 } }, null, 0)
    expect(row(gs, 'Battery', 'Discharge rate').value).toBeNull()
    expect(row(gs, 'Battery', 'Charge rate').value).toBe(30)
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
    expect(e['Battery/Cycles']).toBeUndefined()
    expect(e['Battery/Design capacity']).toBeUndefined()
  })
})

describe('temperatures', () => {
  test('one per part that reports one, named after its group', () => {
    const ts = temperatures(groups(T, null, 0))
    expect(ts.map((x) => x.name)).toEqual(['CPU', 'GPU', 'Memory', 'Chassis', 'Samsung PM9A1'])
    expect(ts.map((x) => x.icon)).toEqual(['cpu', 'gpu', 'memory', 'laptop', 'drive'])
    expect(ts[4].reading.value).toBe(41)
  })
})
