import { describe, expect, test } from 'vitest'
import { chargeTargetPct, duration, onBatterySince, timeLeftMin, timeToFullMin } from '../lib/battery'
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

describe('timeToFullMin', () => {
  test('divides the charge still to go by the average charging power', () => {
    // 20 Wh to go at 20 W is an hour.
    const samples = [at(0, 15), at(60, 25), at(120, 15), at(180, 25)]
    expect(timeToFullMin(samples, 60_000, 80_000)).toBe(60)
  })

  test('starts after the laptop was last unplugged', () => {
    const samples = [at(0, 60), at(60, -10), at(120, 20), at(180, 20), at(240, 20)]
    expect(timeToFullMin(samples, 60_000, 80_000)).toBe(60)
  })

  test('waits for a minute of charging', () => {
    expect(timeToFullMin([at(0, 20), at(30, 20)], 60_000, 80_000)).toBeNull()
  })

  test('has nothing to go at the target', () => {
    const samples = [at(0, 20), at(60, 20), at(120, 20)]
    expect(timeToFullMin(samples, 80_000, 80_000)).toBeNull()
  })
})

describe('chargeTargetPct', () => {
  test('a custom range stops at its upper end', () => {
    expect(chargeTargetPct({ kind: 'Custom', start: 75, stop: 80 })).toBe(80)
  })

  test('the other modes charge to full', () => {
    expect(chargeTargetPct({ kind: 'Standard' })).toBe(100)
    expect(chargeTargetPct({ kind: 'Express' })).toBe(100)
  })
})

describe('onBatterySince', () => {
  const MIN = 60_000

  test('reads the time since unplugging and the charge used', () => {
    expect(onBatterySince({ sinceMs: 0, sincePct: 82 }, 134 * MIN, 44)).toBe('2 h 14 min, −38%')
  })

  test('only the time when the charge at unplugging is unknown', () => {
    expect(onBatterySince({ sinceMs: 0, sincePct: null }, 42 * MIN, 44)).toBe('42 min')
  })

  test('no charge used yet shows only the time', () => {
    expect(onBatterySince({ sinceMs: 0, sincePct: 80.2 }, 3 * MIN, 80)).toBe('3 min')
  })
})
