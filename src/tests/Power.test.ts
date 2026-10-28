import { fireEvent, render, screen, within } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { DgpuReport, EnergyReport, GpuApp, GpuChoice, PowerFlow, SleepReport, Telemetry } from '../lib/types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))

const { default: Power, forget } = await import('../views/full/Power.svelte')
const { toasts } = await import('../lib/toasts.svelte')
const { live } = await import('../lib/telemetry.svelte')

function withPower(power: PowerFlow | null, gpu: Telemetry['gpu'] = { state: 'asleep' }) {
  live.telemetry = {
    tsMs: 0,
    battery: null,
    cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null, coreLoads: [] },
    gpu,
    fans: [],
    mem: null,
    dimmC: null,
    skinC: null,
    power,
    drives: [],
  }
}

function powerRow(label: string) {
  return within(screen.getByRole('region', { name: 'Where the power goes' })).getByText(label).closest('.row') as HTMLElement
}

const BRAVE = 'C:\Program Files\Brave\brave.exe'
const GAME = String.raw`C:\Games\game.exe`

function app(over: Partial<GpuApp> = {}): GpuApp {
  return { name: 'brave', path: BRAVE, bytes: 247e6, choice: 'auto', ...over }
}

let dgpu: DgpuReport | null
let setError: string | null
let picked: string | null
let energy: EnergyReport | string
let sleep: SleepReport | string

const ADMIN = 'Battery history needs administrator rights.'

beforeEach(() => {
  forget()
  live.telemetry = null
  invoke.mockReset()
  toasts.items = []
  setError = null
  picked = null
  dgpu = { name: 'NVIDIA T1200 Laptop GPU', active: true, apps: [app()], rules: [] }
  energy = {
    day: [
      { name: 'Claude', mwh: 9000, screenOffMwh: 0 },
      { name: 'System', mwh: 3000, screenOffMwh: 2000 },
    ],
    week: [{ name: 'brave', mwh: 20000, screenOffMwh: 0 }],
    dayTotalMwh: 12000,
    weekTotalMwh: 40000,
  }
  sleep = {
    sessions: [
      { start: '2026-10-05T20:00:00Z', minutes: 600, drainedMwh: 5923, fullMwh: 59228, deepPct: 99, blocker: null },
      { start: '2026-10-05T01:00:00Z', minutes: 60, drainedMwh: 2961, fullMwh: 59228, deepPct: 82, blocker: 'Windows Update' },
    ],
  }
  invoke.mockImplementation(async (cmd: string, args?: { path: string; choice: GpuChoice }) => {
    if (cmd === 'get_dgpu') return dgpu
    if (cmd === 'get_app_energy') {
      if (typeof energy === 'string') throw energy
      return energy
    }
    if (cmd === 'get_sleep') {
      if (typeof sleep === 'string') throw sleep
      return sleep
    }
    if (cmd === 'pick_app') return picked
    if (cmd === 'set_gpu_choice') {
      if (setError) throw setError
      const { path, choice } = args!
      const a = dgpu!.apps.find((x) => x.path === path)
      if (a) a.choice = choice
      const name = path === GAME ? 'game' : 'brave'
      dgpu!.rules = dgpu!.rules.filter((r) => r.path !== path)
      if (choice !== 'auto') dgpu!.rules.push(app({ name, path, bytes: 0, choice }))
      return
    }
    throw `unexpected ${cmd}`
  })
})

describe('Power', () => {
  test('on battery the draw is split into CPU, NVIDIA GPU and the rest', async () => {
    withPower({ source: 'battery', totalW: 18, cpuW: 6.5, gpuW: 0, restW: 11.5 })
    render(Power)
    const region = screen.getByRole('region', { name: 'Where the power goes' })
    expect(within(region).getByText('18.0 W from the battery')).toBeTruthy()
    expect(within(powerRow('CPU')).getByText('6.5 W')).toBeTruthy()
    expect(within(powerRow('CPU')).getByText('36%')).toBeTruthy()
    expect(within(powerRow('NVIDIA GPU')).getByText('Asleep')).toBeTruthy()
    expect(within(powerRow('NVIDIA GPU')).getByText('0.0 W')).toBeTruthy()
    expect(within(powerRow('Screen, SSD, Wi-Fi and the rest')).getByText('11.5 W')).toBeTruthy()
  })

  test('without an NVIDIA reading its row is left out', async () => {
    withPower({ source: 'battery', totalW: 10, cpuW: 4, gpuW: null, restW: 6 }, { state: 'unavailable' })
    render(Power)
    const region = screen.getByRole('region', { name: 'Where the power goes' })
    expect(within(region).queryByText('NVIDIA GPU')).toBeNull()
    expect(within(powerRow('Screen, SSD, Wi-Fi and the rest')).getByText('6.0 W')).toBeTruthy()
  })

  test('on AC the parts are shown with the adapter rating and no total', async () => {
    withPower(
      { source: 'adapter', cpuW: 25, gpuW: 12, intoBatteryW: 33, adapterW: 130 },
      { state: 'active', tempC: 60, loadPct: 40, clockMhz: null, memClockMhz: null, vramUsedMb: null, vramTotalMb: null },
    )
    render(Power)
    const region = screen.getByRole('region', { name: 'Where the power goes' })
    expect(within(region).getByText('130 W adapter')).toBeTruthy()
    expect(within(powerRow('CPU')).getByText('25.0 W')).toBeTruthy()
    expect(within(powerRow('NVIDIA GPU')).getByText('12.0 W')).toBeTruthy()
    expect(within(powerRow('Into the battery')).getByText('33.0 W')).toBeTruthy()
    expect(within(region).queryByText(/from the battery/)).toBeNull()
    expect(within(region).getByText(/adapter doesn't report/)).toBeTruthy()
  })

  test('without a battery reading the section is left out', async () => {
    withPower(null)
    render(Power)
    expect(screen.queryByRole('region', { name: 'Where the power goes' })).toBeNull()
  })

  test('names the apps keeping the discrete GPU awake', async () => {
    render(Power)
    expect(await screen.findByText('NVIDIA T1200 Laptop GPU')).toBeTruthy()
    expect(screen.getByText('Active')).toBeTruthy()
    expect(screen.getByText(/1 app keeps it awake/)).toBeTruthy()
    const row = screen.getByText('brave').closest('.row') as HTMLElement
    expect(within(row).getByText('247 MB on the GPU')).toBeTruthy()
  })

  test('a running app can be moved to the integrated GPU, then back to Auto', async () => {
    render(Power)
    const row = (await screen.findByText('brave')).closest('.row') as HTMLElement
    expect(within(row).getByRole('button', { name: 'Auto' }).getAttribute('aria-pressed')).toBe('true')
    await fireEvent.click(within(row).getByRole('button', { name: 'Integrated' }))
    expect(invoke).toHaveBeenCalledWith('set_gpu_choice', { path: BRAVE, choice: 'integrated' })
    expect(await within(row).findByText(/Restart brave to switch/)).toBeTruthy()
    expect(within(row).getByRole('button', { name: 'Integrated' }).getAttribute('aria-pressed')).toBe('true')

    await fireEvent.click(within(row).getByRole('button', { name: 'Auto' }))
    expect(invoke).toHaveBeenCalledWith('set_gpu_choice', { path: BRAVE, choice: 'auto' })
  })

  test('apps with a graphics setting are listed even while the GPU sleeps', async () => {
    dgpu = {
      name: 'NVIDIA RTX A2000',
      active: false,
      apps: [],
      rules: [app({ name: 'game', path: GAME, bytes: 0, choice: 'nvidia' })],
    }
    render(Power)
    expect(await screen.findByText('Sleeping')).toBeTruthy()
    expect(screen.getByText(/No app is using it/)).toBeTruthy()
    const row = screen.getByText('game').closest('.row') as HTMLElement
    expect(within(row).getByText(GAME)).toBeTruthy()
    expect(within(row).getByRole('button', { name: 'NVIDIA' }).getAttribute('aria-pressed')).toBe('true')
    await fireEvent.click(within(row).getByRole('button', { name: 'Integrated' }))
    expect(invoke).toHaveBeenCalledWith('set_gpu_choice', { path: GAME, choice: 'integrated' })
  })

  test('no graphics settings yet says how to add one', async () => {
    render(Power)
    expect(await screen.findByText(/No app has a graphics setting/)).toBeTruthy()
  })

  test('an app picked from disk can be set to the NVIDIA GPU', async () => {
    picked = GAME
    render(Power)
    await fireEvent.click(await screen.findByRole('button', { name: 'Add app…' }))
    const row = (await screen.findByText('game')).closest('.row') as HTMLElement
    expect(within(row).getByText(GAME)).toBeTruthy()
    expect(within(row).getByRole('button', { name: 'Auto' }).getAttribute('aria-pressed')).toBe('true')
    await fireEvent.click(within(row).getByRole('button', { name: 'NVIDIA' }))
    expect(invoke).toHaveBeenCalledWith('set_gpu_choice', { path: GAME, choice: 'nvidia' })
    await vi.waitFor(() => expect(screen.getAllByText('game')).toHaveLength(1))
  })

  test('cancelling the file picker adds nothing', async () => {
    render(Power)
    await fireEvent.click(await screen.findByRole('button', { name: 'Add app…' }))
    expect(invoke).toHaveBeenCalledWith('pick_app')
    expect(screen.getByText(/No app has a graphics setting/)).toBeTruthy()
  })

  test('integrated graphics only hides the GPU card', async () => {
    dgpu = null
    render(Power)
    expect(await screen.findByText(/integrated graphics only/)).toBeTruthy()
    expect(screen.queryByText('Active')).toBeNull()
  })

  test('a failed switch shows why', async () => {
    setError = "Windows didn't accept the graphics setting."
    render(Power)
    const row = (await screen.findByText('brave')).closest('.row') as HTMLElement
    await fireEvent.click(within(row).getByRole('button', { name: 'Integrated' }))
    await vi.waitFor(() => expect(toasts.items.map((t) => t.text)).toEqual([setError]))
  })

  test('battery use by app, for the last day and then the week', async () => {
    render(Power)
    const claude = (await screen.findByText('Claude')).closest('.row') as HTMLElement
    expect(within(claude).getByText('9.0 Wh')).toBeTruthy()
    expect(within(claude).getByText('75%')).toBeTruthy()
    const system = screen.getByText('System').closest('.row') as HTMLElement
    expect(within(system).getByText('2.0 Wh with the screen off')).toBeTruthy()

    await fireEvent.click(screen.getByRole('button', { name: '7 days' }))
    const brave = screen.getByText('brave', { selector: '.energy span' }).closest('.row') as HTMLElement
    expect(within(brave).getByText('20.0 Wh')).toBeTruthy()
    expect(within(brave).getByText('50%')).toBeTruthy()
    expect(screen.queryByText('Claude')).toBeNull()
  })

  test('no time on battery says so', async () => {
    energy = { day: [], week: [], dayTotalMwh: 0, weekTotalMwh: 0 }
    render(Power)
    expect(await screen.findByText('No time on battery in this period.')).toBeTruthy()
  })

  test('sleep drain per session and what kept it awake', async () => {
    render(Power)
    expect(await screen.findByText(/loses about 1.4% an hour asleep on battery. That's normal./)).toBeTruthy()
    const night = screen.getByText(/10 h asleep/).closest('.row') as HTMLElement
    expect(within(night).getByText('10% used')).toBeTruthy()
    expect(within(night).getByText('1.0%/h')).toBeTruthy()
    expect(within(night).getByText(/99% deep sleep/)).toBeTruthy()
    const nap = screen.getByText(/1 h asleep/).closest('.row') as HTMLElement
    expect(within(nap).getByText('5.0%/h')).toBeTruthy()
    expect(within(nap).getByText('Kept awake by Windows Update')).toBeTruthy()
  })

  test('heavy sleep drain is called out', async () => {
    sleep = {
      sessions: [
        { start: '2026-10-05T20:00:00Z', minutes: 120, drainedMwh: 5923, fullMwh: 59228, deepPct: 70, blocker: 'NVIDIA T1200' },
      ],
    }
    render(Power)
    expect(await screen.findByText(/loses about 5.0% an hour asleep on battery. That's more than it should/)).toBeTruthy()
  })

  test('without administrator rights both battery sections say so', async () => {
    energy = sleep = ADMIN
    render(Power)
    await vi.waitFor(() => expect(screen.getAllByText(ADMIN)).toHaveLength(2))
    expect(screen.getByText('NVIDIA T1200 Laptop GPU')).toBeTruthy()
  })

  test('refresh reads both reports again', async () => {
    render(Power)
    await fireEvent.click(await screen.findByRole('button', { name: 'Refresh' }))
    await vi.waitFor(() => expect(invoke.mock.calls.filter(([c]) => c === 'get_sleep')).toHaveLength(2))
    expect(invoke.mock.calls.filter(([c]) => c === 'get_app_energy')).toHaveLength(2)
  })
})
