import { fireEvent, render, screen, within } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { AppState, Routine } from '../lib/types'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))

const { default: RoutineEditor } = await import('../views/full/RoutineEditor.svelte')
const { live } = await import('../lib/telemetry.svelte')

const options = { ssid: 'TOTO4', networks: ['Work', 'TOTO4'], bluetooth: ['MX Master'] }

function blank(over: Partial<Routine> = {}): Routine {
  return {
    id: 'n',
    name: '',
    enabled: true,
    conditions: [],
    actions: { thermal: null, battery: null, powerMode: null },
    notify: false,
    restore: false,
    ...over,
  }
}

function open(routine = blank(), isNew = true) {
  const onsave = vi.fn()
  const oncancel = vi.fn()
  const ondelete = vi.fn()
  render(RoutineEditor, { routine, isNew, options, onsave, oncancel, ondelete })
  return { onsave, oncancel, ondelete }
}

const tile = (name: string) =>
  within(screen.getByRole('group', { name: 'Conditions' })).getByRole('button', { name })
const chip = (group: string, name: string) =>
  within(screen.getByRole('group', { name: group })).getByRole('button', { name })
const save = () => screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement
const saved = (onsave: ReturnType<typeof vi.fn>): Routine => onsave.mock.calls.at(-1)![0]

beforeEach(() => {
  live.app = null
})

describe('Routine editor', () => {
  test('pressing tiles adds and removes conditions, kept in tile order', async () => {
    const { onsave } = open()
    await fireEvent.click(tile('Power'))
    await fireEvent.click(tile('Wi-Fi'))
    await fireEvent.click(tile('Display'))
    await fireEvent.click(tile('Display'))
    expect(tile('Display').getAttribute('aria-pressed')).toBe('false')
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    await fireEvent.click(save())
    const r = saved(onsave)
    expect(r.conditions.map((c) => c.kind)).toEqual(['wifi', 'power'])
    expect(r.conditions[0]).toEqual({ kind: 'wifi', ssid: 'TOTO4', connected: true })
  })

  test('Manually is pressed when no condition is, and clears them', async () => {
    open()
    expect(tile('Manually').getAttribute('aria-pressed')).toBe('true')
    await fireEvent.click(tile('Wi-Fi'))
    expect(tile('Manually').getAttribute('aria-pressed')).toBe('false')
    await fireEvent.click(tile('Manually'))
    expect(tile('Manually').getAttribute('aria-pressed')).toBe('true')
    expect(tile('Wi-Fi').getAttribute('aria-pressed')).toBe('false')
    expect(screen.queryByRole('group', { name: 'Wi-Fi network' })).toBeNull()
  })

  test('the name follows the conditions until typed', async () => {
    open()
    const name = screen.getByRole('textbox', { name: 'Name' }) as HTMLInputElement
    expect(name.value).toBe('Manual routine')
    await fireEvent.click(tile('Wi-Fi'))
    expect(name.value).toBe('TOTO4')
    expect(name.maxLength).toBe(60)
    await fireEvent.input(name, { target: { value: 'Office' } })
    await fireEvent.click(tile('Power'))
    expect(name.value).toBe('Office')
  })

  test('the network and how it is joined come from chips', async () => {
    const { onsave } = open()
    await fireEvent.click(tile('Wi-Fi'))
    await fireEvent.click(chip('Wi-Fi network', 'Work'))
    await fireEvent.click(chip('Wi-Fi state', 'Not connected'))
    await fireEvent.click(chip('Thermal mode', 'Cool'))
    await fireEvent.click(save())
    expect(saved(onsave).conditions).toEqual([{ kind: 'wifi', ssid: 'Work', connected: false }])
  })

  test('another network is trimmed and required', async () => {
    const { onsave } = open()
    await fireEvent.click(tile('Wi-Fi'))
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    await fireEvent.click(chip('Wi-Fi network', 'Other'))
    const input = screen.getByRole('textbox', { name: 'Network name' })
    expect(save().disabled).toBe(true)
    expect(screen.getByText('Enter the Wi-Fi network')).toBeTruthy()
    await fireEvent.input(input, { target: { value: '  ' } })
    expect(save().disabled).toBe(true)
    await fireEvent.input(input, { target: { value: ' Lab ' } })
    expect(save().disabled).toBe(false)
    await fireEvent.click(save())
    expect(saved(onsave).conditions).toEqual([{ kind: 'wifi', ssid: 'Lab', connected: true }])
  })

  test('Between asks for an end other than the start', async () => {
    const { onsave } = open()
    await fireEvent.click(tile('Time'))
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    expect(screen.queryByLabelText('Until')).toBeNull()
    await fireEvent.click(chip('When', 'Between'))
    const until = screen.getByLabelText('Until') as HTMLInputElement
    expect(until.value).toBe('17:00')
    await fireEvent.change(until, { target: { value: '08:00' } })
    expect(save().disabled).toBe(true)
    expect(screen.getByText('Pick an end time other than the start')).toBeTruthy()
    await fireEvent.change(until, { target: { value: '18:30' } })
    await fireEvent.click(chip('Days', 'Sat'))
    await fireEvent.click(save())
    expect(saved(onsave).conditions).toEqual([
      { kind: 'time', days: [0, 1, 2, 3, 4, 5], time: '08:00', until: '18:30' },
    ])
  })

  test('an At time has no Undo', async () => {
    const { onsave } = open(blank({ restore: true, actions: { thermal: 'Quiet', battery: null, powerMode: null } }))
    expect(screen.getByRole('checkbox', { name: 'Undo when it stops' })).toBeTruthy()
    await fireEvent.click(tile('Time'))
    expect(screen.queryByRole('checkbox', { name: /Undo/ })).toBeNull()
    await fireEvent.click(save())
    expect(saved(onsave).restore).toBe(false)
  })

  test('battery level and days on AC have presets and Other', async () => {
    const { onsave } = open()
    await fireEvent.click(tile('Battery'))
    await fireEvent.click(tile('Days on AC'))
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    expect(chip('Battery below', '20%').getAttribute('aria-pressed')).toBe('true')
    await fireEvent.click(chip('Battery below', '15%'))
    await fireEvent.click(chip('Plugged in for', 'Other'))
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Days count' }), { target: { value: '10' } })
    await fireEvent.click(save())
    expect(saved(onsave).conditions).toEqual([
      { kind: 'batteryBelow', percent: 15 },
      { kind: 'longAc', days: 10 },
    ])
  })

  test('Then chips set the actions', async () => {
    const { onsave } = open()
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    await fireEvent.click(chip('Battery profile', 'Desk'))
    await fireEvent.click(chip('Windows power mode', 'Efficiency'))
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Notify' }))
    await fireEvent.click(save())
    const r = saved(onsave)
    expect(r.actions).toEqual({ thermal: 'Quiet', battery: 'home', powerMode: 'BestEfficiency' })
    expect(r.notify).toBe(true)
    expect(r.name).toBe('Manual routine')
  })

  test("a routine that changes nothing can't be saved", async () => {
    open()
    expect(save().disabled).toBe(true)
    expect(screen.getByText('Pick something to change, or turn on Notify')).toBeTruthy()
  })

  test('with the modes matched, the power mode offers to match the thermal mode', async () => {
    live.app = { matchPowerMode: true } as AppState
    open()
    expect(chip('Windows power mode', "Don't change")).toBeTruthy()
    await fireEvent.click(chip('Thermal mode', 'Quiet'))
    expect(chip('Windows power mode', 'Match thermal mode (Best power efficiency)')).toBeTruthy()
  })

  test('Delete only when editing; Cancel leaves', async () => {
    const first = open()
    expect(screen.queryByRole('button', { name: 'Delete' })).toBeNull()
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(first.oncancel).toHaveBeenCalled()
    document.body.innerHTML = ''
    const second = open(blank({ name: 'Office', conditions: [{ kind: 'power', plugged: true }] }), false)
    expect(screen.getByRole('region', { name: 'Edit routine' })).toBeTruthy()
    expect((screen.getByRole('textbox', { name: 'Name' }) as HTMLInputElement).value).toBe('Office')
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    expect(second.ondelete).toHaveBeenCalled()
  })
})
