import { duration } from './battery'
import { num, pct, rpm, temp, watts, wh } from './format'
import type { IconName } from './icons'
import type { AppState, DriveSnapshot, Telemetry } from './types'

export interface Reading {
  label: string
  value: number | null
  show: (v: number) => string
  /** The temperature from which the part runs hot. */
  hot?: number
  /** False for a value that barely moves, where a min and max say nothing. */
  extremes?: boolean
}

export interface Group {
  name: string
  icon: IconName
  rows: Reading[]
}

/** Lowest and highest value of each row, keyed `Group/Label`. */
export type Extremes = Record<string, { min: number; max: number }>

/** Where each part runs hot: the CPU throttles near 100 °C, the GPU near 87 °C. */
const HOT = { cpu: 90, gpu: 85, dimm: 80, chassis: 50, drive: 70 }

const MIB = 1024 * 1024
const mhz = (v: number) => num(v, 0, ' MHz')
const gb = (mb: number) => num(mb / 1024, 1, ' GB')
const volts = (v: number) => num(v, 2, ' V')
const minutes = (v: number) => duration(Math.round(v))
export const rate = (bps: number) => (bps < MIB ? num(bps / 1024, 0, ' KB/s') : num(bps / MIB, 1, ' MB/s'))
const pct1 = (v: number) => pct(v, 1)

export const isHot = (r: Reading, v: number) => r.hot != null && v >= r.hot

/** A drive's model, or which disk it is when the model is missing or shared. */
export function driveNames(drives: DriveSnapshot[]): string[] {
  const count = (m: string | null) => drives.filter((d) => d.model === m).length
  return drives.map((d) =>
    d.model == null ? `Disk ${d.index}` : count(d.model) > 1 ? `${d.model} (Disk ${d.index})` : d.model,
  )
}

/** The sensor table: one group per part, the live value of each reading. */
export function groups(t: Telemetry | null, app: AppState | null, nowMs: number): Group[] {
  const gpu = t?.gpu.state === 'active' ? t.gpu : null
  const battery = t?.battery ?? app?.battery ?? null
  const mem = t?.mem
  const stint = app?.onBatterySince
  const drives = t?.drives ?? []
  const names = driveNames(drives)
  return [
    {
      name: 'CPU',
      icon: 'cpu',
      rows: [
        { label: 'Temperature', value: t?.cpu.tempC ?? null, show: temp, hot: HOT.cpu },
        { label: 'Load', value: t?.cpu.loadPct ?? null, show: pct },
        { label: 'Clock', value: t?.cpu.clockMhz ?? null, show: mhz },
        { label: 'Power', value: t?.power?.cpuW ?? null, show: watts },
        ...(t?.cpu.coreLoads ?? []).map((v, i) => ({ label: `CPU ${i}`, value: v, show: pct })),
      ],
    },
    {
      name: 'GPU',
      icon: 'gpu',
      rows: [
        { label: 'Temperature', value: gpu?.tempC ?? null, show: temp, hot: HOT.gpu },
        { label: 'Load', value: gpu?.loadPct ?? null, show: pct },
        { label: 'Core clock', value: gpu?.clockMhz ?? null, show: mhz },
        { label: 'Memory clock', value: gpu?.memClockMhz ?? null, show: mhz },
        { label: 'Memory used', value: gpu?.vramUsedMb ?? null, show: gb },
        { label: 'Power', value: t?.power?.gpuW ?? null, show: watts },
      ],
    },
    {
      name: 'Fans',
      icon: 'fan',
      rows: (t?.fans ?? []).map((f) => ({ label: f.name, value: f.rpm, show: rpm })),
    },
    {
      name: 'Memory',
      icon: 'memory',
      rows: [
        { label: 'Module temperature', value: t?.dimmC ?? null, show: temp, hot: HOT.dimm },
        { label: 'Used', value: mem?.usedMb ?? null, show: gb },
        { label: 'Load', value: mem && mem.totalMb > 0 ? (mem.usedMb / mem.totalMb) * 100 : null, show: pct },
      ],
    },
    {
      name: 'Chassis',
      icon: 'laptop',
      rows: [{ label: 'Temperature', value: t?.skinC ?? null, show: temp, hot: HOT.chassis }],
    },
    ...drives.map(
      (d, i): Group => ({
        name: names[i],
        icon: 'drive',
        rows: [
          { label: 'Temperature', value: d.tempC, show: temp, hot: HOT.drive },
          { label: 'Activity', value: d.activePct, show: pct },
          { label: 'Read rate', value: d.readBps, show: rate },
          { label: 'Write rate', value: d.writeBps, show: rate },
        ],
      }),
    ),
    {
      name: 'Battery',
      icon: 'battery',
      rows: [
        { label: 'Charge', value: battery?.percent ?? null, show: pct },
        { label: 'Charge rate', value: battery?.state === 'charging' ? battery.powerW : null, show: watts },
        { label: 'Discharge rate', value: battery?.state === 'discharging' ? -battery.powerW : null, show: watts },
        { label: 'Voltage', value: battery?.voltageV ?? null, show: volts },
        { label: 'Remaining capacity', value: battery?.remainingMwh ?? null, show: wh },
        { label: 'Full charge capacity', value: battery?.fullMwh ?? null, show: wh, extremes: false },
        { label: 'Design capacity', value: battery?.designMwh ?? null, show: wh, extremes: false },
        { label: 'Wear level', value: battery?.wearPct ?? null, show: pct1, extremes: false },
        { label: 'Cycles', value: battery?.cycles ?? null, show: (v) => num(v), extremes: false },
        {
          label: 'On battery',
          value: stint ? Math.max(0, Math.floor((nowMs - stint.sinceMs) / 60_000)) : null,
          show: minutes,
          extremes: false,
        },
      ],
    },
  ]
}

/** The temperature of each part that has one, under its group's name and icon. */
export function temperatures(gs: Group[]): { name: string; icon: IconName; reading: Reading }[] {
  return gs.flatMap((g) => g.rows.filter((r) => r.hot != null).map((reading) => ({ name: g.name, icon: g.icon, reading })))
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
