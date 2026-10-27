import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import type { AppState, Telemetry } from '../lib/types'

const invoke = vi.fn()
const handlers: Record<string, (e: { payload: unknown }) => void> = {}

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: () => Promise.resolve('0.1.0') }))
const checkUpdate = vi.fn()
const relaunch = vi.fn()
vi.mock('@tauri-apps/plugin-updater', () => ({ check: () => checkUpdate() }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: () => relaunch() }))
const openUrl = vi.fn()
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: (url: string) => openUrl(url) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, h: (e: { payload: unknown }) => void) => {
    handlers[name] = h
    return () => delete handlers[name]
  },
}))

const { default: FullWindow } = await import('../views/FullWindow.svelte')
const { live } = await import('../lib/telemetry.svelte')

function appState(over: Partial<AppState> = {}): AppState {
  return {
    charge: { kind: 'Custom', start: 75, stop: 80 },
    activeProfile: { kind: 'known', profile: 'home' },
    thermal: 'Optimized',
    profiles: {
      home: { kind: 'Custom', start: 75, stop: 80 },
      campus: { kind: 'Standard' },
      storage: { kind: 'Custom', start: 50, stop: 60 },
    },
    battery: {
      percent: 82,
      state: 'discharging',
      powerW: -12.5,
      remainingMwh: 70000,
      fullMwh: 85000,
      designMwh: 97000,
      wearPct: 12.4,
      voltageV: 12.6,
      cycles: null,
    },
    availability: { cctk: true, wmi: true, dcm: true, admin: true, optimizerRunning: false },
    autostart: false,
    optimizerWarningDismissed: false,
    peripherals: [],
    ...over,
  }
}

const TELEMETRY: Telemetry = {
  tsMs: 0,
  battery: null,
  cpu: { loadPct: 7, tempC: 54 },
  gpu: { state: 'active', tempC: 60, loadPct: 3 },
  fans: [],
  mem: { usedMb: 10240, totalMb: 32768 },
  dimmC: null,
  skinC: null,
  power: null,
}

function route(
  state: AppState,
  device: { serviceTag?: string | null } = {},
  telemetry: Telemetry | null = TELEMETRY,
  onReady: () => void = () => {},
) {
  invoke.mockImplementation((cmd: string, args?: { enabled?: boolean }) => {
    if (cmd === 'get_state') return Promise.resolve(state)
    if (cmd === 'get_telemetry') return Promise.resolve(telemetry)
    if (cmd === 'get_history' || cmd === 'get_health_log') return Promise.resolve([])
    if (cmd === 'set_autostart') return Promise.resolve(args?.enabled ?? false)
    if (cmd === 'window_ready') return Promise.resolve(onReady())
    if (cmd === 'get_app_name') return Promise.resolve('MyXPS')
    if (cmd === 'get_device_info')
      return Promise.resolve({
        manufacturer: 'Dell Inc.',
        model: 'Precision 5560',
        serviceTag: 'ABC1234',
        biosVersion: '1.47.0',
        biosDate: '05/31/2026',
        ...device,
      })
    return Promise.reject(`unexpected ${cmd}`)
  })
}

async function open(section: string) {
  await waitFor(() => expect(screen.getByRole('radio', { name: 'Desk' }).getAttribute('aria-checked')).toBe('true'))
  await fireEvent.click(screen.getByRole('button', { name: section }))
}

const openBattery = () => open('Battery')

beforeEach(() => {
  invoke.mockReset()
})

afterEach(() => live.stop())

describe('FullWindow', () => {
  test('reports ready even when loading fails, so the error is not hidden', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'get_state' ? Promise.reject('no state') : Promise.resolve([]),
    )
    render(FullWindow)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
  })

  test('reveals only once telemetry, device and history are on screen, so nothing pops in', async () => {
    const seen: { model: boolean; telemetry: boolean; history: boolean } = { model: false, telemetry: false, history: false }
    route(appState(), {}, TELEMETRY, () => {
      seen.model = screen.queryByText('Precision 5560') !== null
      seen.telemetry = live.telemetry !== null
      seen.history = invoke.mock.calls.some(([c]) => c === 'get_history')
    })
    render(FullWindow)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
    expect(seen).toEqual({ model: true, telemetry: true, history: true })
  })

  test('waits for the first telemetry tick when none is cached', async () => {
    let telemetryAtReady: Telemetry | null = null
    route(appState(), {}, null, () => (telemetryAtReady = live.telemetry))
    render(FullWindow)
    await waitFor(() => expect(handlers.telemetry).toBeTruthy())
    await new Promise((r) => setTimeout(r, 50))
    expect(invoke).not.toHaveBeenCalledWith('window_ready')
    handlers.telemetry({ payload: { ...TELEMETRY, tsMs: 1 } })
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
    expect(telemetryAtReady).toMatchObject({ tsMs: 1 })
  })

  test('cycles hidden when null', async () => {
    route(appState())
    render(FullWindow)
    await openBattery()
    expect(await screen.findByText('Wear')).toBeTruthy()
    expect(screen.queryByText('Cycles')).toBeNull()
  })

  test('cycles shown when reported', async () => {
    const s = appState()
    route({ ...s, battery: { ...s.battery!, cycles: 214 } })
    render(FullWindow)
    await openBattery()
    expect(await screen.findByText('Cycles')).toBeTruthy()
    expect(screen.getByText('214')).toBeTruthy()
  })

  test('wear shows "13.6%" for wearPct 13.6', async () => {
    const s = appState()
    route({ ...s, battery: { ...s.battery!, wearPct: 13.6 } })
    render(FullWindow)
    await openBattery()
    expect(await screen.findByText('13.6%')).toBeTruthy()
  })

  test('navigation lists every section', async () => {
    route(appState())
    render(FullWindow)
    for (const name of ['Overview', 'Battery', 'Performance', 'Sensors', 'Settings']) {
      expect(screen.getByRole('button', { name })).toBeTruthy()
    }
  })

  test('the device card shows the model and service tag', async () => {
    route(appState())
    render(FullWindow)
    expect(await screen.findByText('Precision 5560')).toBeTruthy()
    expect(screen.getByText('ABC1234')).toBeTruthy()
  })

  test('overview explains each thermal mode', async () => {
    route(appState())
    render(FullWindow)
    expect(await screen.findByText('Balanced default')).toBeTruthy()
    expect(screen.getByRole('radio', { name: 'Ultra Performance' })).toBeTruthy()
  })

  test('overview hero shows the charge and the active profile', async () => {
    route(appState())
    render(FullWindow)
    expect(await screen.findByText('On battery · Desk profile')).toBeTruthy()
  })

  test('performance page holds the thermal mode', async () => {
    route(appState())
    render(FullWindow)
    await open('Performance')
    expect(screen.getByRole('heading', { level: 1, name: 'Performance' })).toBeTruthy()
    expect(screen.getByRole('radiogroup', { name: 'Thermal mode' })).toBeTruthy()
    expect(screen.queryByRole('radiogroup', { name: 'Battery profile' })).toBeNull()
  })

  test('settings page shows the version and BIOS', async () => {
    route(appState())
    render(FullWindow)
    await open('Settings')
    expect(await screen.findByText('0.1.0')).toBeTruthy()
    expect(screen.getByText('1.47.0')).toBeTruthy()
  })

  test('settings page names the app after the laptop line', async () => {
    route(appState())
    render(FullWindow)
    await open('Settings')
    expect(await screen.findByText('MyXPS')).toBeTruthy()
    expect(screen.getByText('MyXPS updates')).toBeTruthy()
  })

  test('warranty button opens Dell support for the service tag', async () => {
    route(appState())
    render(FullWindow)
    await open('Settings')
    await fireEvent.click(await screen.findByRole('button', { name: 'Open on Dell.com' }))
    expect(openUrl).toHaveBeenCalledWith('https://www.dell.com/support/home/en-us/product-support/servicetag/ABC1234')
  })

  test('warranty button is hidden without a service tag', async () => {
    route(appState(), { serviceTag: null })
    render(FullWindow)
    await open('Settings')
    expect(await screen.findByText('1.47.0')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Open on Dell.com' })).toBeNull()
  })

  test('autostart toggle calls setAutostart(true)', async () => {
    route(appState())
    render(FullWindow)
    await open('Settings')
    const toggle = await screen.findByRole('switch', { name: 'Start at sign-in' })
    await waitFor(() => expect((toggle as HTMLButtonElement).disabled).toBe(false))
    await fireEvent.click(toggle)
    expect(invoke).toHaveBeenCalledWith('set_autostart', { enabled: true })
    await waitFor(() => expect(toggle.getAttribute('aria-checked')).toBe('true'))
  })

  test('check for updates reports an up-to-date app', async () => {
    route(appState())
    checkUpdate.mockResolvedValue(null)
    render(FullWindow)
    await open('Settings')
    await fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }))
    expect(await screen.findByText("You're up to date")).toBeTruthy()
  })

  test('an available update installs and relaunches', async () => {
    route(appState())
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined)
    checkUpdate.mockResolvedValue({ version: '0.2.0', downloadAndInstall })
    render(FullWindow)
    await open('Settings')
    await fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }))
    expect(await screen.findByText('Version 0.2.0 is available')).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Install and restart' }))
    await waitFor(() => expect(relaunch).toHaveBeenCalled())
    expect(downloadAndInstall).toHaveBeenCalled()
  })

  test('a failed update check shows the error', async () => {
    route(appState())
    checkUpdate.mockRejectedValue(new Error('offline'))
    render(FullWindow)
    await open('Settings')
    await fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }))
    expect(await screen.findByText(/Couldn't check for updates/)).toBeTruthy()
  })

  test('history from getHistory is requested for 30 minutes', async () => {
    route(appState())
    render(FullWindow)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_history', { minutes: 30 }))
  })
})
