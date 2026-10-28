import type { HistorySample } from './types'

const WINDOW_MS = 5 * 60_000
const MIN_SPAN_MS = 60_000

/** Minutes left on battery at the average draw of the last 5 minutes since unplugging; `null` until a minute of it. */
export function timeLeftMin(samples: HistorySample[], remainingMwh: number): number | null {
  const last = samples.at(-1)
  if (!last) return null
  const draws: number[] = []
  let first = last.tsMs
  for (let i = samples.length - 1; i >= 0; i--) {
    const s = samples[i]
    if (s.batteryW == null || s.batteryW >= 0 || last.tsMs - s.tsMs > WINDOW_MS) break
    draws.push(-s.batteryW)
    first = s.tsMs
  }
  if (last.tsMs - first < MIN_SPAN_MS) return null
  const avgW = draws.reduce((a, b) => a + b, 0) / draws.length
  return Math.round((remainingMwh / 1000 / avgW) * 60)
}

/** `3 h 08 min`, or `42 min` under an hour. */
export function duration(min: number): string {
  const h = Math.floor(min / 60)
  const m = min % 60
  return h ? `${h} h ${String(m).padStart(2, '0')} min` : `${m} min`
}
