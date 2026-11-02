import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { Routine } from '../lib/types'

const invoke = vi.fn()
const handlers: Record<string, (e: { payload: unknown }) => void> = {}
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, h: (e: { payload: unknown }) => void) => {
    handlers[name] = h
    return () => delete handlers[name]
  },
}))

const { default: Automation } = await import('../views/full/Automation.svelte')
const { toasts } = await import('../lib/toasts.svelte')
const { live } = await import('../lib/telemetry.svelte')

function routine(over: Partial<Routine> = {}): Routine {
  return {
    id: 'a',
    name: 'Office',
    enabled: true,
    conditions: [{ kind: 'wifi', ssid: 'Work', connected: true }],
    actions: { thermal: 'UltraPerformance', battery: null, powerMode: null },
    notify: false,
    restore: false,
    ...over,
  }
}

let stored: Routine[]
let running: string[]
function route(saveError?: string) {
  invoke.mockImplementation(async (cmd: string, args?: { routines: Routine[] }) => {
    if (cmd === 'get_routines') return structuredClone(stored)
    if (cmd === 'routine_status') return { running }
    if (cmd === 'routine_options') return { ssid: 'TOTO4', networks: ['TOTO4', 'Work'], bluetooth: ['MX Master'] }
    if (cmd === 'start_routine' || cmd === 'stop_routine') return undefined
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

const card = (name: string) => screen.getByRole('group', { name })

beforeEach(() => {
  invoke.mockReset()
  toasts.items = []
  live.app = null
  stored = []
  running = []
  route()
})

describe('Routines page', () => {
  test('each routine is a card with its conditions and what it changes', async () => {
    stored = [routine({ notify: true })]
    render(Automation)
    await screen.findByText('Office')
    const office = card('Office')
    expect(within(office).getByText('Work')).toBeTruthy()
    expect(within(office).getByText('Ultra Performance')).toBeTruthy()
    expect(within(office).getByRole('switch', { name: 'Office' })).toBeTruthy()
    expect(within(office).queryByText('Running')).toBeNull()
  })

  test('a running routine says so and can be stopped', async () => {
    stored = [routine()]
    running = ['a']
    render(Automation)
    expect(await screen.findByText('Running')).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Stop Office' }))
    expect(invoke).toHaveBeenCalledWith('stop_routine', { id: 'a' })
  })

  test('a manual routine offers Start', async () => {
    stored = [routine({ id: 'm', name: 'Focus', conditions: [] })]
    render(Automation)
    await screen.findByText('Focus')
    expect(within(card('Focus')).getByText('Manually')).toBeTruthy()
    await fireEvent.click(screen.getByRole('button', { name: 'Start Focus' }))
    expect(invoke).toHaveBeenCalledWith('start_routine', { id: 'm' })
    // An automatic routine has no Start.
    expect(screen.queryByRole('button', { name: 'Start Office' })).toBeNull()
  })

  test('the running label follows the routine-status event', async () => {
    stored = [routine()]
    render(Automation)
    await screen.findByText('Office')
    await waitFor(() => expect(handlers['routine-status']).toBeTruthy())
    handlers['routine-status']({ payload: { running: ['a'] } })
    expect(await screen.findByText('Running')).toBeTruthy()
    handlers['routine-status']({ payload: { running: [] } })
    await waitFor(() => expect(screen.queryByText('Running')).toBeNull())
  })

  test('says when there are no routines', async () => {
    render(Automation)
    expect(await screen.findByText(/No routines yet/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'New routine' })).toBeTruthy()
  })

  test('a new routine is added at the end', async () => {
    stored = [routine()]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'New routine' }))
    expect(screen.getByRole('region', { name: 'New routine' })).toBeTruthy()
    const tiles = screen.getByRole('group', { name: 'Conditions' })
    await fireEvent.click(within(tiles).getByRole('button', { name: 'Power' }))
    await fireEvent.click(within(screen.getByRole('group', { name: 'Thermal mode' })).getByRole('button', { name: 'Quiet' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saved().length).toBe(2))
    expect(saved()[1].conditions).toEqual([{ kind: 'power', plugged: true }])
    expect(saved()[1].name).toBe('Plugged in')
    expect(await screen.findByText('Plugged in', { selector: '.name' })).toBeTruthy()
  })

  test('Cancel goes back without saving', async () => {
    stored = [routine()]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'New routine' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(await screen.findByText('Office')).toBeTruthy()
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'set_routines')).toBe(false)
  })

  test('editing a routine replaces it in place', async () => {
    stored = [routine(), routine({ id: 'b', name: 'Home' })]
    render(Automation)
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit Office' }))
    expect(screen.getByRole('region', { name: 'Edit routine' })).toBeTruthy()
    await fireEvent.click(within(screen.getByRole('group', { name: 'Thermal mode' })).getByRole('button', { name: 'Cool' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saved().map((r) => r.id)).toEqual(['a', 'b']))
    expect(saved()[0].actions.thermal).toBe('Cool')
    expect(saved()[0].name).toBe('Office')
  })

  test('routines move, switch off and delete', async () => {
    stored = [routine(), routine({ id: 'b', name: 'Home' })]
    render(Automation)
    expect(((await screen.findByRole('button', { name: 'Move Office up' })) as HTMLButtonElement).disabled).toBe(true)
    expect((screen.getByRole('button', { name: 'Move Home down' }) as HTMLButtonElement).disabled).toBe(true)

    await fireEvent.click(screen.getByRole('button', { name: 'Move Home up' }))
    await waitFor(() => expect(saved().map((r) => r.id)).toEqual(['b', 'a']))

    await fireEvent.click(screen.getByRole('switch', { name: 'Office' }))
    await waitFor(() => expect(saved().find((r) => r.id === 'a')!.enabled).toBe(false))

    await fireEvent.click(screen.getByRole('button', { name: 'Edit Home' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(saved().map((r) => r.id)).toEqual(['a']))
  })

  test('a failed save shows the error and reloads what is saved', async () => {
    stored = [routine()]
    route('Could not save settings: disk full')
    render(Automation)
    await fireEvent.click(await screen.findByRole('switch', { name: 'Office' }))
    await waitFor(() => expect(toasts.items.map((t) => t.text)).toContain('Could not save settings: disk full'))
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Office' }).getAttribute('aria-checked')).toBe('true'))
  })
})
