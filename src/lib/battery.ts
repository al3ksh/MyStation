import type { BatteryStint, ChargeCfg, HistorySample } from './types'

const WINDOW_MS = 5 * 60_000
const MIN_SPAN_MS = 60_000

/**
 * Average power over the last 5 minutes the battery kept flowing one way: `-1` draining, `1` charging.
 * `null` until there's a minute of it, so a fresh plug or unplug doesn't swing the estimate.
 */
function averageW(samples: HistorySample[], sign: 1 | -1): number | null {
  const last = samples.at(-1)
  if (!last) return null
  const watts: number[] = []
  let first = last.tsMs
  for (let i = samples.length - 1; i >= 0; i--) {
    const s = samples[i]
    if (s.batteryW == null || s.batteryW * sign <= 0 || last.tsMs - s.tsMs > WINDOW_MS) break
    watts.push(s.batteryW * sign)
    first = s.tsMs
  }
  if (last.tsMs - first < MIN_SPAN_MS) return null
  return watts.reduce((a, b) => a + b, 0) / watts.length
}

/** Minutes left on battery at the recent average draw. */
export function timeLeftMin(samples: HistorySample[], remainingMwh: number): number | null {
  const w = averageW(samples, -1)
  return w == null ? null : Math.round((remainingMwh / 1000 / w) * 60)
}

/** Minutes to `targetMwh` at the recent average charging power; charging slows near the top, so it runs short. */
export function timeToFullMin(samples: HistorySample[], remainingMwh: number, targetMwh: number): number | null {
  const w = averageW(samples, 1)
  if (w == null || remainingMwh >= targetMwh) return null
  return Math.round(((targetMwh - remainingMwh) / 1000 / w) * 60)
}

/** Where charging stops: the top of a custom range, otherwise full. */
export function chargeTargetPct(cfg: ChargeCfg): number {
  return cfg.kind === 'Custom' ? cfg.stop : 100
}

/** `3 h 08 min`, or `42 min` under an hour. */
export function duration(min: number): string {
  const h = Math.floor(min / 60)
  const m = min % 60
  return h ? `${h} h ${String(m).padStart(2, '0')} min` : `${m} min`
}

/** `2 h 14 min, −38%`: the time since unplugging and the charge used since, when known. */
export function onBatterySince(stint: BatteryStint, nowMs: number, percent: number): string {
  const time = duration(Math.max(0, Math.floor((nowMs - stint.sinceMs) / 60_000)))
  const used = stint.sincePct == null ? 0 : Math.round(stint.sincePct - percent)
  return used > 0 ? `${time}, −${used}%` : time
}
