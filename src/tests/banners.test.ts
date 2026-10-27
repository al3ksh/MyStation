import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import type { AppState } from '../lib/types'

const invoke = vi.fn()
const handlers: Record<string, (e: { payload: unknown }) => void> = {}

// The cached tick is plumbing for the first paint; tests drive telemetry through events.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, ...a: unknown[]) => (cmd === 'get_telemetry' ? Promise.resolve(null) : invoke(cmd, ...a)),
}))
const openUrl = vi.fn()
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: (url: string) => openUrl(url) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, h: (e: { payload: unknown }) => void) => {
    handlers[name] = h
    return () => delete handlers[name]
  },
}))

const { default: Flyout } = await import('../views/Flyout.svelte')
const { live } = await import('../lib/telemetry.svelte')
const { BANNER_TEXT, bannersFor } = await import('../lib/banners')

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
    powerMode: 'Balanced',
    energySaver: false,
    matchPowerMode: false,
    updateVersion: null,
    ...over,
  }
}

const avail = (over: Partial<AppState['availability']>) => ({
  ...appState().availability,
  ...over,
})

beforeEach(() => {
  invoke.mockReset()
})

afterEach(() => live.stop())

describe('banners', () => {
  test('optimizer banner hidden after dismissal', () => {
    const running = appState({ availability: avail({ optimizerRunning: true }) })
    expect(bannersFor(running)).toEqual(['optimizer'])
    expect(bannersFor({ ...running, optimizerWarningDismissed: true })).toEqual([])
  })

  test('order is noAdmin, noCctk, optimizer, noDcm', () => {
    const s = appState({ availability: { cctk: false, wmi: true, dcm: false, admin: false, optimizerRunning: true } })
    expect(bannersFor(s)).toEqual(['noAdmin', 'noCctk', 'optimizer', 'noDcm'])
  })

  test('a WMI failure is reported as such, not as a missing Dell Command | Monitor', () => {
    expect(bannersFor(appState({ availability: avail({ wmi: false, dcm: false }) }))).toEqual(['noWmi'])
  })

  test('noCctk says what needs Dell Command | Configure', () => {
    expect(BANNER_TEXT.noCctk.body).toContain('Battery profiles, thermal modes and BIOS settings')
  })

  test.each([
    ['noCctk', 'Dell Command | Configure not found', 'https://www.dell.com/support/kbdoc/en-us/000178000/dell-command-configure'],
    ['noDcm', 'Dell Command | Monitor not found', 'https://www.dell.com/support/kbdoc/en-us/000177080/dell-command-monitor'],
  ])('%s offers the Dell download page', async (_id, title, url) => {
    const availability = _id === 'noCctk' ? avail({ cctk: false }) : avail({ dcm: false })
    invoke.mockResolvedValue(appState({ availability }))
    openUrl.mockReset().mockResolvedValue(undefined)
    render(Flyout)
    expect(await screen.findByText(title)).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Download' }))
    expect(openUrl).toHaveBeenCalledWith(url)
    expect(invoke).not.toHaveBeenCalledWith('dismiss_optimizer_warning')
  })

  test('noCctk disables mode controls', async () => {
    invoke.mockResolvedValue(appState({ availability: avail({ cctk: false }) }))
    render(Flyout)
    expect(await screen.findByText('Dell Command | Configure not found')).toBeTruthy()
    const group = screen.getByRole('radiogroup', { name: 'Battery profile' })
    expect(group.getAttribute('aria-disabled')).toBe('true')
    await fireEvent.click(screen.getByRole('radio', { name: 'Storage' }))
    expect(invoke).not.toHaveBeenCalledWith('set_battery_profile', expect.anything())
  })

  test('flyout shows only the first banner', async () => {
    invoke.mockResolvedValue(appState({ availability: avail({ admin: false, cctk: false }) }))
    render(Flyout)
    expect(await screen.findByText('Not running as administrator')).toBeTruthy()
    expect(screen.queryByText('Dell Command | Configure not found')).toBeNull()
  })

  test('"Don’t show again" calls dismissOptimizerWarning and hides the banner', async () => {
    const s = appState({ availability: avail({ optimizerRunning: true }) })
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_state') return Promise.resolve(s)
      if (cmd === 'dismiss_optimizer_warning') {
        handlers['state-changed']({ payload: { ...s, optimizerWarningDismissed: true } })
        return Promise.resolve()
      }
      return Promise.reject(`unexpected ${cmd}`)
    })
    render(Flyout)
    await fireEvent.click(await screen.findByRole('button', { name: 'Don’t show again' }))
    expect(invoke).toHaveBeenCalledWith('dismiss_optimizer_warning')
    await waitFor(() => expect(screen.queryByText('Dell Optimizer is running')).toBeNull())
  })
})
