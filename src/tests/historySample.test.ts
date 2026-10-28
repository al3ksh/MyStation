import { describe, expect, test, vi } from 'vitest'
import type { Telemetry } from '../lib/types'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const { toSample } = await import('../lib/history.svelte')

const T: Telemetry = {
  tsMs: 5,
  battery: null,
  cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null, coreLoads: [] },
  gpu: { state: 'active', tempC: 60, loadPct: 40, clockMhz: null, memClockMhz: null, vramUsedMb: null, vramTotalMb: null },
  fans: [],
  mem: null,
  dimmC: null,
  skinC: null,
  power: null,
  drives: [],
}

describe('toSample', () => {
  test('keeps the GPU load for its chart, and none while the card sleeps', () => {
    expect(toSample(T).gpuLoad).toBe(40)
    expect(toSample({ ...T, gpu: { state: 'asleep' } }).gpuLoad).toBeNull()
  })
})
