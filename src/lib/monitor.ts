import { duration } from './battery'
import { num, pct, rpm, temp, watts } from './format'
import type { AppState, Records, Telemetry } from './types'

export interface Reading {
  label: string
  value: number | null
  show: (v: number) => string
  /** The all-time record this row is compared against. */
  record?: keyof Records
  /** False for a value that only grows, where a min and max say nothing. */
  extremes?: boolean
}

export interface Group {
  name: string
  rows: Reading[]
  /** A record that spans the group's rows, such as the fastest of any fan. */
  record?: keyof Records
}

/** Lowest and highest value of each row, keyed `Group/Label`. */
export type Extremes = Record<string, { min: number; max: number }>

const mhz = (v: number) => num(v, 0, ' MHz')
const gb = (mb: number) => num(mb / 1024, 1, ' GB')
const volts = (v: number) => num(v, 2, ' V')
const minutes = (v: number) => duration(Math.round(v))

/** The sensor table: one group per part, the live value of each reading. */
export function groups(t: Telemetry | null, app: AppState | null, nowMs: number): Group[] {
  const gpu = t?.gpu.state === 'active' ? t.gpu : null
  const battery = t?.battery ?? app?.battery ?? null
  const stint = app?.onBatterySince
  return [
    {
      name: 'CPU',
      rows: [
        { label: 'Temperature', value: t?.cpu.tempC ?? null, show: temp, record: 'cpuTempC' },
        { label: 'Load', value: t?.cpu.loadPct ?? null, show: pct },
        { label: 'Clock', value: t?.cpu.clockMhz ?? null, show: mhz },
        { label: 'Power', value: t?.power?.cpuW ?? null, show: watts },
      ],
    },
    {
      name: 'GPU',
      rows: [
        { label: 'Temperature', value: gpu?.tempC ?? null, show: temp, record: 'gpuTempC' },
        { label: 'Load', value: gpu?.loadPct ?? null, show: pct },
        { label: 'Power', value: t?.power?.gpuW ?? null, show: watts },
      ],
    },
    {
      name: 'Fans',
      record: 'fanRpm',
      rows: (t?.fans ?? []).map((f) => ({ label: f.name, value: f.rpm, show: rpm })),
    },
    {
      name: 'Memory',
      rows: [
        { label: 'Module temperature', value: t?.dimmC ?? null, show: temp },
        { label: 'Used', value: t?.mem?.usedMb ?? null, show: gb },
      ],
    },
    {
      name: 'Chassis',
      rows: [{ label: 'Temperature', value: t?.skinC ?? null, show: temp }],
    },
    {
      name: 'Battery',
      rows: [
        { label: 'Charge', value: battery?.percent ?? null, show: pct },
        {
          label: 'Discharge rate',
          value: battery?.state === 'discharging' ? -battery.powerW : null,
          show: watts,
          record: 'batteryDrawW',
        },
        { label: 'Voltage', value: battery?.voltageV ?? null, show: volts },
        {
          label: 'On battery',
          value: stint ? Math.max(0, Math.floor((nowMs - stint.sinceMs) / 60_000)) : null,
          show: minutes,
          record: 'batteryStintMin',
          extremes: false,
        },
      ],
    },
  ]
}

/** `prev` widened by the readings of one tick. */
export function track(prev: Extremes, gs: Group[]): Extremes {
  const next = { ...prev }
  for (const g of gs) {
    for (const r of g.rows) {
      if (r.value == null || !Number.isFinite(r.value) || r.extremes === false) continue
      const key = `${g.name}/${r.label}`
      const e = next[key]
      next[key] = e ? { min: Math.min(e.min, r.value), max: Math.max(e.max, r.value) } : { min: r.value, max: r.value }
    }
  }
  return next
}

/** How a record shows in its row's own unit; the stint is minutes. */
export const recordShow: Record<keyof Records, (v: number) => string> = {
  cpuTempC: temp,
  gpuTempC: temp,
  fanRpm: rpm,
  batteryDrawW: watts,
  batteryStintMin: minutes,
}
