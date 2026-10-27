// Mirrors the Rust types serialized by src-tauri (camelCase).

export type BatteryProfile = 'home' | 'campus' | 'storage'
export type ThermalMode = 'Optimized' | 'Cool' | 'Quiet' | 'UltraPerformance'

export type ChargeCfg =
  | { kind: 'Standard' }
  | { kind: 'Adaptive' }
  | { kind: 'PrimAcUse' }
  | { kind: 'Express' }
  | { kind: 'Custom'; start: number; stop: number }

export type ActiveProfile = { kind: 'known'; profile: BatteryProfile } | { kind: 'other'; cfg: ChargeCfg }

export interface Profiles {
  home: ChargeCfg
  campus: ChargeCfg
  storage: ChargeCfg
}

export type PowerState = 'charging' | 'discharging' | 'bypass'

export interface BatterySnapshot {
  percent: number | null
  state: PowerState
  /** Positive while charging, negative while discharging. */
  powerW: number
  remainingMwh: number
  fullMwh: number | null
  designMwh: number | null
  wearPct: number | null
  cycles: number | null
  voltageV: number
}

export interface Availability {
  cctk: boolean
  wmi: boolean
  dcm: boolean
  admin: boolean
  optimizerRunning: boolean
}

export interface AppState {
  charge: ChargeCfg | null
  activeProfile: ActiveProfile | null
  thermal: ThermalMode | null
  profiles: Profiles
  battery: BatterySnapshot | null
  availability: Availability
  autostart: boolean
  optimizerWarningDismissed: boolean
  /** Connected Bluetooth devices that report a battery, emptiest first. */
  peripherals: Peripheral[]
}

export type PeripheralKind = 'mouse' | 'keyboard' | 'headphones' | 'speaker' | 'controller' | 'pen' | 'phone' | 'other'

export interface Peripheral {
  name: string
  kind: PeripheralKind
  batteryPct: number
}

export type GpuSnapshot =
  | { state: 'asleep' }
  | { state: 'unavailable' }
  | { state: 'active'; tempC: number; loadPct: number }

export interface FanReading {
  name: string
  rpm: number
}

/** Battery: the parts add up to the total. Adapter: the adapter's own draw isn't reported. */
export type PowerFlow =
  | { source: 'battery'; totalW: number; cpuW: number | null; gpuW: number | null; restW: number | null }
  | { source: 'adapter'; cpuW: number | null; gpuW: number | null; intoBatteryW: number; adapterW: number | null }

export interface Telemetry {
  tsMs: number
  battery: BatterySnapshot | null
  cpu: { loadPct: number | null; tempC: number | null }
  gpu: GpuSnapshot
  fans: FanReading[]
  mem: { usedMb: number; totalMb: number } | null
  dimmC: number | null
  skinC: number | null
  power: PowerFlow | null
}

export interface HistorySample {
  tsMs: number
  cpuTemp: number | null
  cpuLoad: number | null
  gpuTemp: number | null
  batteryPct: number | null
  batteryW: number | null
}

export interface HealthEntry {
  date: string
  fullMwh: number
  designMwh: number
  cycles: number | null
}

/** SMBIOS identity; any field may be missing on odd firmware. */
export interface DeviceInfo {
  manufacturer: string | null
  model: string | null
  serviceTag: string | null
  biosVersion: string | null
  biosDate: string | null
}

export interface ScheduleEntry {
  /** 0 = Monday … 6 = Sunday. */
  days: number[]
  /** Local time, HH:MM. */
  time: string
  profile: BatteryProfile
}

export interface Automation {
  thermal: { enabled: boolean; onAc: ThermalMode | null; onBattery: ThermalMode | null }
  schedule: { enabled: boolean; entries: ScheduleEntry[] }
  longAc: { enabled: boolean; days: number; profile: BatteryProfile }
  notify: boolean
}

export type BiosGroup = 'Keyboard' | 'Power' | 'Devices' | 'Startup'

export interface BiosChoice {
  value: string
  label: string
}

/** One curated BIOS setting as cctk reports it; `value` is null when the BIOS did not report it. */
export type BiosSetting = {
  key: string
  label: string
  description: string
  group: BiosGroup
  /** Shown only while this setting is not `Disabled`. */
  parent: string | null
  value: string | null
} & ({ kind: 'toggle' } | { kind: 'choice'; choices: BiosChoice[] } | { kind: 'number'; min: number; max: number })

/** The NVMe SMART / Health Information log. */
export interface SmartLog {
  criticalWarning: number
  tempC: number | null
  availableSparePct: number
  spareThresholdPct: number
  /** The vendor's estimate of rated endurance consumed; may exceed 100. */
  percentUsed: number
  bytesRead: number
  bytesWritten: number
  powerCycles: number
  powerOnHours: number
  unsafeShutdowns: number
  mediaErrors: number
}

export interface DriveReport {
  model: string | null
  nvme: boolean
  smart: SmartLog | null
  yearsLeft: number | null
  /** ISO date of the first reading the forecast measures from. */
  trackingSince: string | null
}

export interface StorageReport {
  drives: DriveReport[]
  /** Total and free bytes of the Windows volume. */
  volume: [number, number] | null
}

export interface BootRecord {
  start: string
  totalMs: number
  mainPathMs: number
  postBootMs: number
  startupApps: number
}

export interface Culprit {
  name: string
  kind: 'app' | 'driver' | 'service' | 'device'
  boots: number
  avgDelayMs: number
}

export interface BootReport {
  /** Newest first. */
  boots: BootRecord[]
  /** Worst first. */
  culprits: Culprit[]
  /** Firmware (POST) time of the latest power-on, Task Manager's "BIOS time". */
  firmwareMs: number | null
  /** Shutdowns resume a hibernated kernel, and Windows doesn't time those boots. */
  fastStartup: boolean
}

/** Which GPU an app runs on; `auto` leaves it to Windows. */
export type GpuChoice = 'auto' | 'integrated' | 'nvidia'

export interface GpuApp {
  name: string
  path: string
  /** Memory held on the discrete GPU; 0 for apps listed only by their preference. */
  bytes: number
  /** Takes effect the next time the app starts. */
  choice: GpuChoice
}

export interface DgpuReport {
  name: string | null
  /** Powered on; when off, no app holds it. */
  active: boolean
  /** Apps keeping it awake, by memory held. */
  apps: GpuApp[]
  /** Apps with a choice other than Auto, by name. */
  rules: GpuApp[]
}

export interface AppEnergy {
  name: string
  mwh: number
  /** The part used while the screen was off. */
  screenOffMwh: number
}

/** Battery energy per app as Windows estimates it; the discrete GPU isn't counted. */
export interface EnergyReport {
  /** Most energy first. */
  day: AppEnergy[]
  week: AppEnergy[]
  /** Across all apps, not just the ones listed. */
  dayTotalMwh: number
  weekTotalMwh: number
}

export interface SleepSession {
  start: string
  minutes: number
  drainedMwh: number
  fullMwh: number
  /** Share of the session in the deepest hardware sleep state. */
  deepPct: number
  /** What kept it awake the longest, when deep sleep fell short. */
  blocker: string | null
}

export interface SleepReport {
  /** On battery, newest first. */
  sessions: SleepSession[]
}

export interface UsbDevice {
  name: string
  arrivedMs: number | null
  /** Selectively suspended right now rather than fully powered. */
  suspended: boolean
  /** Extra battery draw measured when it was plugged in, in watts. */
  drawW: number | null
  speed: UsbSpeed | null
  /** Set when a USB 3 device runs at USB 2 speed: the cable, or a USB 2 port or hub. */
  bottleneck: 'cable' | 'port' | null
}

export type UsbSpeed = 'low' | 'full' | 'high' | 'super' | 'superPlus'

export interface UsbReport {
  /** Removable devices, biggest draw first. */
  devices: UsbDevice[]
  builtIn: number
}

export interface DisplayMode {
  width: number
  height: number
  hz: number
}

export type DisplayConnection = 'internal' | 'displayPort' | 'hdmi' | 'dvi' | 'vga' | 'usb' | 'wireless' | 'other'

export interface DisplayInfo {
  /** The monitor's own name from its EDID, else what Windows calls it. */
  name: string | null
  connection: DisplayConnection
  current: DisplayMode
  /** A sharper or faster mode the monitor supports; `setting` when Windows offers it, `link` when the connection can't carry it. */
  upgrade: { mode: DisplayMode; limit: 'setting' | 'link' } | null
}
