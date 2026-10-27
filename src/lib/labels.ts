import type { ActiveProfile, BatteryProfile, ChargeCfg, PowerMode, ThermalMode } from './types'

export const PROFILE_LABEL: Record<BatteryProfile, string> = {
  home: 'Desk',
  campus: 'Remote',
  storage: 'Storage',
}

export const THERMAL_LABEL: Record<ThermalMode, string> = {
  Optimized: 'Optimized',
  Cool: 'Cool',
  Quiet: 'Quiet',
  UltraPerformance: 'Ultra Performance',
}

export const POWER_MODE_LABEL: Record<PowerMode, string> = {
  BestEfficiency: 'Best power efficiency',
  Balanced: 'Balanced',
  BestPerformance: 'Best performance',
}

/** The Windows power mode that goes with a thermal mode when the two are matched. */
export function powerModeFor(mode: ThermalMode): PowerMode {
  if (mode === 'UltraPerformance') return 'BestPerformance'
  if (mode === 'Optimized') return 'Balanced'
  return 'BestEfficiency'
}

const BIOS_MODE_LABEL: Record<Exclude<ChargeCfg['kind'], 'Custom'>, string> = {
  Standard: 'Standard',
  Adaptive: 'Adaptive',
  PrimAcUse: 'Primarily AC',
  Express: 'Express',
}

export function chargeCfgLabel(c: ChargeCfg): string {
  if (c.kind === 'Custom') return `${c.start}–${c.stop}%`
  if (c.kind === 'Standard') return 'Standard (up to 100%)'
  return BIOS_MODE_LABEL[c.kind]
}

/** Compact form for a tile subtitle. */
export function chargeRangeLabel(c: ChargeCfg): string {
  if (c.kind === 'Custom') return `${c.start}–${c.stop}%`
  if (c.kind === 'Standard') return 'Up to 100%'
  return BIOS_MODE_LABEL[c.kind]
}

export function activeProfileLabel(a: ActiveProfile | null): string {
  if (!a) return '—'
  if (a.kind === 'known') return PROFILE_LABEL[a.profile]
  if (a.cfg.kind === 'Custom') return `Custom (${a.cfg.start}–${a.cfg.stop}%)`
  return BIOS_MODE_LABEL[a.cfg.kind]
}
