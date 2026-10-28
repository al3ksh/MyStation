import { render, screen, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import type { Records, Telemetry } from '../lib/types'

const invoke = vi.fn()
const handlers: Record<string, (e: { payload: unknown }) => void> = {}
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, h: (e: { payload: unknown }) => void) => {
    handlers[name] = h
    return () => delete handlers[name]
  },
}))

const { default: Monitor } = await import('../views/Monitor.svelte')
const { live } = await import('../lib/telemetry.svelte')

const AT = new Date(2026, 9, 14, 12).getTime()
const EMPTY: Records = { cpuTempC: null, gpuTempC: null, fanRpm: null, batteryDrawW: null, batteryStintMin: null }

const T: Telemetry = {
  tsMs: 0,
  battery: null,
  cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null },
  gpu: { state: 'asleep' },
  fans: [{ name: 'CPU fan', rpm: 2400 }],
  mem: null,
  dimmC: null,
  skinC: null,
  power: null,
}

function route(records: Records | Promise<Records>) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_records') return Promise.resolve(records)
    if (cmd === 'get_state') return Promise.resolve({ battery: null, onBatterySince: null })
    if (cmd === 'get_telemetry') return Promise.resolve(T)
    if (cmd === 'window_ready') return Promise.resolve()
    return Promise.reject(`unexpected ${cmd}`)
  })
}

/** The cells of the row whose first cell is `label`, within `group`'s rows. */
function cells(group: string, label: string) {
  const rows = screen.getAllByRole('row')
  const start = rows.findIndex((r) => r.textContent?.startsWith(group))
  const row = rows.slice(start + 1).find((r) => within(r).queryAllByRole('cell')[0]?.textContent === label)!
  return within(row).getAllByRole('cell').map((c) => c.textContent?.trim())
}

beforeEach(() => {
  invoke.mockReset()
})
afterEach(() => live.stop())

describe('Monitor', () => {
  test('shows each sensor with its value, min, max and record', async () => {
    route({ ...EMPTY, cpuTempC: { value: 97.4, atMs: AT } })
    render(Monitor)
    await waitFor(() => expect(cells('CPU', 'Temperature')).toEqual(['Temperature', '54 °C', '54 °C', '54 °C', '97 °C']))
    expect(screen.getByRole('columnheader', { name: 'Record' })).toBeTruthy()
  })

  test('min and max follow the readings since the window opened', async () => {
    route(EMPTY)
    render(Monitor)
    await waitFor(() => expect(cells('CPU', 'Temperature')[1]).toBe('54 °C'))
    handlers['telemetry']({ payload: { ...T, cpu: { ...T.cpu, tempC: 81 } } })
    // A tick a second, as the poller sends them.
    await waitFor(() => expect(cells('CPU', 'Temperature')[1]).toBe('81 °C'))
    handlers['telemetry']({ payload: { ...T, cpu: { ...T.cpu, tempC: 66 } } })
    await waitFor(() => expect(cells('CPU', 'Temperature')).toEqual(['Temperature', '66 °C', '54 °C', '81 °C', '—']))
  })

  test('a record shows its date on hover', async () => {
    route({ ...EMPTY, cpuTempC: { value: 97.4, atMs: AT } })
    render(Monitor)
    const cell = await screen.findByText('97 °C')
    expect(cell.getAttribute('title')).toMatch(/^Oct 14, 2026/)
  })

  test('the fan record sits on the Fans row', async () => {
    route({ ...EMPTY, fanRpm: { value: 5432, atMs: AT } })
    render(Monitor)
    const fans = (await screen.findByText('5,432 RPM')).closest('tr')!
    expect(fans.textContent).toContain('Fans')
  })

  test('an asleep GPU shows dashes, not zeros', async () => {
    route(EMPTY)
    render(Monitor)
    await waitFor(() => expect(cells('GPU', 'Temperature')).toEqual(['Temperature', '—', '—', '—', '—']))
  })

  test('reports ready even when the records fail to load', async () => {
    route(Promise.reject('no records'))
    render(Monitor)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
    expect(cells('CPU', 'Temperature')[1]).toBe('54 °C')
  })

  test('says when temperatures and fans are recorded', async () => {
    route(EMPTY)
    render(Monitor)
    expect(await screen.findByText(/only while a window is open/)).toBeTruthy()
  })
})
