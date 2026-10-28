import { render, screen, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import type { Telemetry } from '../lib/types'

const invoke = vi.fn()
const handlers: Record<string, (e: { payload: unknown }) => void> = {}
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, h: (e: { payload: unknown }) => void) => {
    handlers[name] = h
    return () => delete handlers[name]
  },
}))

import { fireEvent } from '@testing-library/svelte'

const { default: Monitor } = await import('../views/Monitor.svelte')
const { live } = await import('../lib/telemetry.svelte')

const T: Telemetry = {
  tsMs: 0,
  battery: null,
  cpu: { loadPct: 7, tempC: 54, clockMhz: 1972, limitPct: null, coreLoads: [12, 30] },
  gpu: { state: 'asleep' },
  fans: [{ name: 'CPU fan', rpm: 2400 }],
  mem: null,
  dimmC: null,
  skinC: null,
  power: null,
  drives: [],
}

function route() {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_state') return Promise.resolve({ battery: null, onBatterySince: null })
    if (cmd === 'get_telemetry') return Promise.resolve(T)
    if (cmd === 'window_ready') return Promise.resolve()
    return Promise.reject(`unexpected ${cmd}`)
  })
}

/** The cells of the row whose first cell is `label`, within `group`'s rows. */
function cells(group: string, label: string) {
  const rows = screen.getAllByRole('row')
  const start = rows.findIndex((r) => r.querySelector('th[scope=rowgroup]')?.textContent?.endsWith(group))
  const row = rows.slice(start + 1).find((r) => within(r).queryAllByRole('cell')[0]?.textContent === label)!
  return within(row).getAllByRole('cell')
}
const texts = (group: string, label: string) => cells(group, label).map((c) => c.textContent?.trim())

beforeEach(() => {
  localStorage.clear()
  invoke.mockReset()
  route()
})
afterEach(() => live.stop())

describe('Monitor', () => {
  test('shows each sensor with its value, min and max', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')).toEqual(['Temperature', '54 °C', '54 °C', '54 °C']))
    expect(screen.queryByRole('columnheader', { name: 'Record' })).toBeNull()
  })

  test('min and max follow the readings since the window opened', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    handlers['telemetry']({ payload: { ...T, cpu: { ...T.cpu, tempC: 81 } } })
    // A tick a second, as the poller sends them.
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('81 °C'))
    handlers['telemetry']({ payload: { ...T, cpu: { ...T.cpu, tempC: 66 } } })
    await waitFor(() => expect(texts('CPU', 'Temperature')).toEqual(['Temperature', '66 °C', '54 °C', '81 °C']))
  })

  test('a hot temperature shows in red, and so does its max', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    expect(cells('CPU', 'Temperature')[1].classList.contains('hot')).toBe(false)
    handlers['telemetry']({ payload: { ...T, cpu: { ...T.cpu, tempC: 95 } } })
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('95 °C'))
    const [, value, min, max] = cells('CPU', 'Temperature')
    expect(value.classList.contains('hot')).toBe(true)
    expect(min.classList.contains('hot')).toBe(false)
    expect(max.classList.contains('hot')).toBe(true)
  })

  test('each group carries its icon', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    const header = screen.getAllByRole('row').find((r) => r.textContent?.endsWith('Fans'))!
    expect(header.querySelector('.icon')).toBeTruthy()
  })

  test('an asleep GPU shows dashes, not zeros', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('GPU', 'Temperature')).toEqual(['Temperature', '—', '—', '—']))
  })

  test('reports the window ready once it has painted', async () => {
    render(Monitor)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
    expect(texts('CPU', 'Temperature')[1]).toBe('54 °C')
  })

  test('has no note under the table', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    expect(screen.queryByText(/window opened|recorded/)).toBeNull()
  })

  test('a group folds away and back from its header', async () => {
    render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    const toggle = screen.getByRole('button', { name: 'CPU' })
    expect(toggle.getAttribute('aria-expanded')).toBe('true')
    await fireEvent.click(toggle)
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByText('Clock')).toBeNull()
    await fireEvent.click(toggle)
    expect(screen.getByText('Clock')).toBeTruthy()
  })

  test('the load of each thread starts folded, and stays as left on the next open', async () => {
    const { unmount } = render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    const threads = screen.getByRole('button', { name: 'CPU threads' })
    expect(threads.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByText('CPU 0')).toBeNull()
    await fireEvent.click(threads)
    expect(texts('CPU threads', 'CPU 1')[1]).toBe('30%')
    unmount()
    render(Monitor)
    await waitFor(() => expect(texts('CPU threads', 'CPU 0')[1]).toBe('12%'))
  })

  test('the column headers sit outside the scrolling rows', async () => {
    const { container } = render(Monitor)
    await waitFor(() => expect(texts('CPU', 'Temperature')[1]).toBe('54 °C'))
    const scroller = container.querySelector('.rows')!
    expect(scroller.textContent).toContain('Temperature')
    expect(scroller.contains(screen.getByRole('columnheader', { name: 'Sensor' }))).toBe(false)
  })
})
