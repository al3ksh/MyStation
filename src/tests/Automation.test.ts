import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { AppState, Routine } from '../lib/types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))

const { default: Automation } = await import('../views/full/Automation.svelte')
const { toasts } = await import('../lib/toasts.svelte')
const { live } = await import('../lib/telemetry.svelte')

function routine(over: Partial<Routine> = {}): Routine {
  return {
    id: 'a',
    name: 'Office',
    enabled: true,
    trigger: { kind: 'wifi', ssid: 'Work', connected: true },
    actions: { thermal: 'UltraPerformance', battery: null, powerMode: null },
    notify: false,
    restore: false,
    ...over,
  }
}

let stored: Routine[]
function route(saveError?: string) {
  invoke.mockImplementation(async (cmd: string, args?: { routines: Routine[] }) => {
    if (cmd === 'get_routines') return structuredClone(stored)
    if (cmd === 'routine_options') return { ssid: 'TOTO4', networks: ['TOTO4', 'Work'], bluetooth: ['MX Master'] }
    if (cmd === 'set_routines') {
      if (saveError) throw saveError
      stored = structuredClone(args!.routines)
      return stored
    }
    throw new Error(`unexpected ${cmd}`)
  })
}

function saved(): Routine[] {
  const calls = invoke.mock.calls.filter(([cmd]) => cmd === 'set_routines')
  return calls.at(-1)![1].routines
}

beforeEach(() => {
  invoke.mockReset()
  toasts.items = []
  live.app = null
  stored = []
  route()
})

describe('Routines page', () => {
  test('lists each routine with what it does', async () => {
    stored = [routine()]
    render(Automation)
    expect(await screen.findByText('Office')).toBeTruthy()
    expect(screen.getByText('Connected to Work → Ultra Performance')).toBeTruthy()
  })

  test('says when there are no routines', async () => {
    render(Automation)
    expect(await screen.findByText(/No routines yet/)).toBeTruthy()
  })

  test('the wizard builds a Wi-Fi routine on the current network', async () => {
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: /New routine/ }))
    await waitFor(() => expect((screen.getByLabelText('Network') as HTMLInputElement).value).toBe('TOTO4'))

    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    // Nothing chosen yet: the routine would do nothing.
    expect((screen.getByRole('button', { name: 'Next' }) as HTMLButtonElement).disabled).toBe(true)
    await fireEvent.change(screen.getByLabelText('Thermal mode'), { target: { value: 'Quiet' } })
    await fireEvent.change(screen.getByLabelText('Battery profile'), { target: { value: 'home' } })
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))

    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Connected to TOTO4')
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(saved()).toEqual([
        expect.objectContaining({
          name: 'Connected to TOTO4',
          enabled: true,
          trigger: { kind: 'wifi', ssid: 'TOTO4', connected: true },
          actions: { thermal: 'Quiet', battery: 'home', powerMode: null },
          notify: false,
        }),
      ]),
    )
    expect(await screen.findByText('Connected to TOTO4 → Quiet · Desk profile')).toBeTruthy()
  })

  test('a routine can restore what it changed when its trigger ends', async () => {
    stored = [routine()]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit Office' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    await fireEvent.click(screen.getByRole('switch', { name: 'Restore when it ends' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    expect(screen.getByText('Connected to Work → Ultra Performance · Restores')).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saved()[0].restore).toBe(true))
  })

  test('a time trigger has no end to restore at', async () => {
    stored = [routine({ restore: true })]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit Office' }))
    await fireEvent.change(screen.getByLabelText('Trigger'), { target: { value: 'time' } })
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    expect(screen.queryByRole('switch', { name: 'Restore when it ends' })).toBeNull()
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saved()[0].restore).toBe(false))
  })

  test('a time trigger needs at least one day', async () => {
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: /New routine/ }))
    await fireEvent.change(screen.getByLabelText('Trigger'), { target: { value: 'time' } })
    for (const day of ['Mon', 'Tue', 'Wed', 'Thu', 'Fri']) {
      await fireEvent.click(screen.getByRole('button', { name: day }))
    }
    expect(screen.getByText('Pick at least one day')).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Next' }) as HTMLButtonElement).disabled).toBe(true)
  })

  test('with the modes matched, leaving the power mode says what it will follow', async () => {
    live.app = { matchPowerMode: true } as AppState
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: /New routine/ }))
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    await fireEvent.change(screen.getByLabelText('Thermal mode'), { target: { value: 'UltraPerformance' } })
    const keep = screen.getByLabelText('Windows power mode').querySelector('option')!
    expect(keep.textContent).toBe('Match thermal mode (Best performance)')
  })

  test('editing a routine replaces it in place', async () => {
    stored = [routine(), routine({ id: 'b', name: 'Home' })]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit Office' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    await fireEvent.click(screen.getByRole('switch', { name: 'Show a notification' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Office')
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saved().map((r) => [r.id, r.notify])).toEqual([['a', true], ['b', false]]))
  })

  test('routines move, switch off and delete', async () => {
    stored = [routine(), routine({ id: 'b', name: 'Home' })]
    render(Automation)
    expect(((await screen.findByRole('button', { name: 'Move Office up' })) as HTMLButtonElement).disabled).toBe(true)

    await fireEvent.click(screen.getByRole('button', { name: 'Move Home up' }))
    await waitFor(() => expect(saved().map((r) => r.id)).toEqual(['b', 'a']))

    await fireEvent.click(screen.getByRole('switch', { name: 'Office' }))
    await waitFor(() => expect(saved().find((r) => r.id === 'a')!.enabled).toBe(false))

    await fireEvent.click(screen.getByRole('button', { name: 'Delete Home' }))
    await waitFor(() => expect(saved().map((r) => r.id)).toEqual(['a']))
  })

  test('a failed save shows the error and reloads what is saved', async () => {
    stored = [routine()]
    route('Could not save settings: disk full')
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'Delete Office' }))
    await waitFor(() => expect(toasts.items.map((t) => t.text)).toContain('Could not save settings: disk full'))
    expect(await screen.findByText('Office')).toBeTruthy()
  })
})
