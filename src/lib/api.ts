import { invoke } from '@tauri-apps/api/core'
import type { AppState, BatteryProfile, BiosSetting, BootReport, DeviceInfo, DgpuReport, DisplayInfo, EnergyReport, GpuChoice, HealthEntry, HistorySample, Routine, RoutineOptions, SleepReport, Spec, StorageReport, Telemetry, ThermalMode, UsbReport } from './types'

export const api = {
  getState: () => invoke<AppState>('get_state'),
  setBatteryProfile: (profile: BatteryProfile) => invoke<AppState>('set_battery_profile', { profile }),
  setThermalMode: (mode: ThermalMode) => invoke<AppState>('set_thermal_mode', { mode }),
  setMatchPowerMode: (enabled: boolean) => invoke<AppState>('set_match_power_mode', { enabled }),
  /** The last telemetry tick, or null while the poller has none yet. */
  getTelemetry: () => invoke<Telemetry | null>('get_telemetry'),
  getHistory: (minutes: number) => invoke<HistorySample[]>('get_history', { minutes }),
  getHealthLog: () => invoke<HealthEntry[]>('get_health_log'),
  setAutostart: (enabled: boolean) => invoke<boolean>('set_autostart', { enabled }),
  dismissOptimizerWarning: () => invoke<void>('dismiss_optimizer_warning'),
  /** Resizes the flyout to its content height (CSS px), keeping it anchored above the tray. */
  fitFlyout: (height: number) => invoke<void>('fit_flyout', { height }),
  getDeviceInfo: () => invoke<DeviceInfo>('get_device_info'),
  getAppName: () => invoke<string>('get_app_name'),
  getRoutines: () => invoke<Routine[]>('get_routines'),
  setRoutines: (routines: Routine[]) => invoke<Routine[]>('set_routines', { routines }),
  routineOptions: () => invoke<RoutineOptions>('routine_options'),
  /** One cctk run; takes several seconds. */
  getBiosSettings: () => invoke<BiosSetting[]>('get_bios_settings'),
  setBiosSetting: (key: string, value: string) => invoke<void>('set_bios_setting', { key, value }),
  getStorage: () => invoke<StorageReport>('get_storage'),
  /** Needs administrator rights to read the boot log. */
  getBoot: () => invoke<BootReport>('get_boot'),
  getUsb: () => invoke<UsbReport>('get_usb'),
  getDisplays: () => invoke<DisplayInfo[]>('get_displays'),
  getSpec: () => invoke<Spec>('get_spec'),
  /** Null on machines with integrated graphics only. */
  getDgpu: () => invoke<DgpuReport | null>('get_dgpu'),
  /** Applies the next time the app starts. */
  setGpuChoice: (path: string, choice: GpuChoice) => invoke<void>('set_gpu_choice', { path, choice }),
  /** A file dialog for an .exe; null when cancelled. */
  pickApp: () => invoke<string | null>('pick_app'),
  /** Needs administrator rights; runs powercfg, so it takes a few seconds. */
  getAppEnergy: () => invoke<EnergyReport>('get_app_energy'),
  /** Needs administrator rights; runs powercfg. */
  getSleep: () => invoke<SleepReport>('get_sleep'),
  /** `section` opens the window on that page. */
  openFullWindow: (section?: string) =>
    section ? invoke<void>('open_full_window', { section }) : invoke<void>('open_full_window'),
  /** The calling window has painted its first state; the backend shows it only now, so it never flashes blank. */
  windowReady: () => invoke<void>('window_ready'),
  /** The sensor monitor window, over the main window. */
  openMonitor: () => invoke<void>('open_monitor'),
}
