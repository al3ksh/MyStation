import { describe, expect, test } from 'vitest'
import { duration, timeLeftMin } from '../lib/battery'
import type { HistorySample } from '../lib/types'

const at = (s: number, batteryW: number | null): HistorySample => ({
  tsMs: s * 1000,
  cpuTemp: null,
  cpuLoad: null,
  gpuTemp: null,
  batteryPct: null,
  batteryW,
})

describe('timeLeftMin', () => {
  test('divides the charge left by the average draw', () => {
    // 10 W and 20 W average to 15 W; 30 Wh lasts 2 hours.
    const samples = [at(0, -10), at(60, -20), at(120, -10), at(180, -20)]
    expect(timeLeftMin(samples, 30_000)).toBe(120)
  })

  test('averages only the last five minutes', () => {
    const samples = [at(0, -50), at(400, -10), at(500, -10), at(600, -10)]
    expect(timeLeftMin(samples, 10_000)).toBe(60)
  })

  test('starts after the laptop was last plugged in', () => {
    const samples = [at(0, -40), at(60, 15), at(120, -10), at(180, -10), at(240, -10)]
    expect(timeLeftMin(samples, 10_000)).toBe(60)
  })

  test('waits for a minute on battery', () => {
    expect(timeLeftMin([at(0, -10), at(30, -10)], 10_000)).toBeNull()
  })

  test('has no estimate while charging', () => {
    expect(timeLeftMin([at(0, 15), at(60, 15), at(120, 15)], 10_000)).toBeNull()
  })
})

describe('duration', () => {
  test('reads as hours and minutes', () => {
    expect(duration(188)).toBe('3 h 08 min')
    expect(duration(120)).toBe('2 h 00 min')
  })

  test('under an hour drops the hours', () => {
    expect(duration(42)).toBe('42 min')
  })
})
