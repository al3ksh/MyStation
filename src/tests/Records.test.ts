import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { Records } from '../lib/types'

const invoke = vi.fn()
const close = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ close }) }))

const { default: RecordsView } = await import('../views/Records.svelte')

const AT = new Date(2026, 9, 14, 12).getTime()
const EMPTY: Records = { cpuTempC: null, gpuTempC: null, fanRpm: null, batteryDrawW: null, batteryStintMin: null }

function route(records: Records | Promise<Records>) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_records') return Promise.resolve(records)
    if (cmd === 'window_ready') return Promise.resolve()
    return Promise.reject(`unexpected ${cmd}`)
  })
}

beforeEach(() => {
  invoke.mockReset()
  close.mockReset()
})

describe('Records', () => {
  test('lists each record with its value and date', async () => {
    route({
      cpuTempC: { value: 97.4, atMs: AT },
      gpuTempC: { value: 81, atMs: AT },
      fanRpm: { value: 5432, atMs: AT },
      batteryDrawW: { value: 64.25, atMs: AT },
      batteryStintMin: { value: 188, atMs: AT },
    })
    render(RecordsView)
    expect(await screen.findByText('97 °C')).toBeTruthy()
    expect(screen.getByText('81 °C')).toBeTruthy()
    expect(screen.getByText('5,432 RPM')).toBeTruthy()
    expect(screen.getByText('64.3 W')).toBeTruthy()
    expect(screen.getByText('3 h 08 min')).toBeTruthy()
    expect(screen.getAllByText('Oct 14, 2026')).toHaveLength(5)
  })

  test('a record not set yet says so', async () => {
    route({ ...EMPTY, cpuTempC: { value: 90, atMs: AT } })
    render(RecordsView)
    expect(await screen.findByText('90 °C')).toBeTruthy()
    expect(screen.getAllByText('Not yet')).toHaveLength(4)
  })

  test('says when temperatures and fans are recorded', async () => {
    route(EMPTY)
    render(RecordsView)
    expect(await screen.findByText(/only while a window is open/)).toBeTruthy()
  })

  test('reports ready once the records are on screen', async () => {
    route({ ...EMPTY, fanRpm: { value: 4000, atMs: AT } })
    render(RecordsView)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
    expect(screen.getByText('4,000 RPM')).toBeTruthy()
  })

  test('reports ready even when loading fails, so the window still shows', async () => {
    route(Promise.reject('no records'))
    render(RecordsView)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('window_ready'))
  })

  test('Esc closes the window', async () => {
    route(EMPTY)
    render(RecordsView)
    await fireEvent.keyDown(window, { key: 'Escape' })
    expect(close).toHaveBeenCalled()
  })
})
