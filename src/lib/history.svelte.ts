import { api } from './api'
import type { HistorySample, Telemetry } from './types'

export const HISTORY_MINUTES = 30

/** Sample of one telemetry tick, in the shape `get_history` returns. */
export function toSample(t: Telemetry): HistorySample {
  return {
    tsMs: t.tsMs,
    cpuTemp: t.cpu.tempC,
    cpuLoad: t.cpu.loadPct,
    gpuTemp: t.gpu.state === 'active' ? t.gpu.tempC : null,
    gpuLoad: t.gpu.state === 'active' ? t.gpu.loadPct : null,
    batteryPct: t.battery?.percent ?? null,
    batteryW: t.battery?.powerW ?? null,
  }
}

/** Last 30 minutes for the full window's charts: loaded once, then extended by telemetry. */
class HistoryStore {
  samples = $state<HistorySample[]>([])

  async load() {
    const past = await api.getHistory(HISTORY_MINUTES)
    // Ticks that arrived while loading are newer than anything in `past`.
    const last = past.at(-1)?.tsMs ?? -Infinity
    this.samples = [...past, ...this.samples.filter((s) => s.tsMs > last)]
  }

  append(t: Telemetry) {
    const cutoff = t.tsMs - HISTORY_MINUTES * 60_000
    this.samples = [...this.samples.filter((s) => s.tsMs >= cutoff), toSample(t)]
  }

  clear() {
    this.samples = []
  }
}

export const history = new HistoryStore()
