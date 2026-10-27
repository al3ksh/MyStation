import { fireEvent, render, screen } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { BootReport, DriveReport, SmartLog, StorageReport, UsbReport } from '../lib/types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
const openUrl = vi.fn()
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: (url: string) => openUrl(url) }))

const { default: Hardware, forget } = await import('../views/full/Hardware.svelte')
const { toasts } = await import('../lib/toasts.svelte')

function smart(over: Partial<SmartLog> = {}): SmartLog {
  return {
    criticalWarning: 0,
    tempC: 41,
    availableSparePct: 100,
    spareThresholdPct: 10,
    percentUsed: 3,
    bytesRead: 20.5e12,
    bytesWritten: 15.4e12,
    powerCycles: 1234,
    powerOnHours: 5678,
    unsafeShutdowns: 42,
    mediaErrors: 0,
    ...over,
  }
}

function drive(over: Partial<DriveReport> = {}): DriveReport {
  return { model: 'Micron 2300 NVMe 1TB', nvme: true, smart: smart(), yearsLeft: null, trackingSince: '2026-10-08', ...over }
}

const BOOT: BootReport = {
  boots: [
    { start: '2026-10-05T08:00:00Z', totalMs: 30000, mainPathMs: 18000, postBootMs: 12000, startupApps: 15 },
    { start: '2026-10-01T19:41:06Z', totalMs: 51577, mainPathMs: 26077, postBootMs: 25500, startupApps: 15 },
  ],
  culprits: [{ name: 'WavesSysSvc Service Application', kind: 'app', boots: 2, avgDelayMs: 12033 }],
  firmwareMs: 6374,
  fastStartup: false,
}

let storage: StorageReport
let boot: BootReport | Error
let usb: UsbReport

function route() {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_storage') return storage
    if (cmd === 'get_usb') return usb
    if (cmd === 'get_boot') {
      if (boot instanceof Error) throw boot.message
      return boot
    }
    throw `unexpected ${cmd}`
  })
}

beforeEach(() => {
  forget()
  invoke.mockReset()
  openUrl.mockReset()
  toasts.items = []
  storage = { drives: [drive()], volume: [1e12, 250e9] }
  boot = BOOT
  usb = { devices: [], builtIn: 0 }
  route()
})

describe('Hardware', () => {
  test('shows drive health, wear and lifetime counters', async () => {
    render(Hardware)
    expect(await screen.findByText('Micron 2300 NVMe 1TB')).toBeTruthy()
    expect(screen.getByText('Healthy')).toBeTruthy()
    expect(screen.getByText('3% of rated endurance')).toBeTruthy()
    expect(screen.getByText('15.4 TB')).toBeTruthy()
    expect(screen.getByText('5,678 h')).toBeTruthy()
    expect(screen.getByText('41 °C')).toBeTruthy()
    expect(screen.getByText(/after a week of tracking \(tracking since Oct 8, 2026\)/)).toBeTruthy()
    expect(screen.getByText('Windows (C:): 250 GB free of 1.0 TB')).toBeTruthy()
  })

  test.each([
    [{ yearsLeft: 7.4 }, {}, 'About 7 years left at your current write rate.'],
    [{ yearsLeft: 1.25 }, {}, 'About 1.3 years left at your current write rate.'],
    [{ yearsLeft: 80 }, {}, 'More than 20 years left at your current write rate.'],
    [{}, { percentUsed: 0 }, 'Less than 1% of rated endurance used.'],
    [{ yearsLeft: 0 }, { percentUsed: 104 }, 'Past its rated endurance. It may keep working, but plan a replacement.'],
  ])('forecast %#', async (d, s, text) => {
    storage = { drives: [drive({ ...d, smart: smart(s) })], volume: null }
    render(Hardware)
    expect(await screen.findByText(text)).toBeTruthy()
  })

  test('warns about a failing drive', async () => {
    storage = { drives: [drive({ smart: smart({ mediaErrors: 3 }) })], volume: null }
    render(Hardware)
    expect(await screen.findByText('3 unrecovered read errors recorded.')).toBeTruthy()
    expect(screen.queryByText('Healthy')).toBeNull()
  })

  test('explains a drive without health data', async () => {
    storage = { drives: [drive({ smart: null }), drive({ model: 'USB disk', nvme: false, smart: null })], volume: null }
    render(Hardware)
    expect(await screen.findByText('Health data needs administrator rights.')).toBeTruthy()
    expect(screen.getByText('Health data is available for NVMe drives only.')).toBeTruthy()
  })

  test('shows boot times and what slowed them down', async () => {
    render(Hardware)
    expect(await screen.findByText(/to the desktop, last full restart Oct 5/)).toBeTruthy()
    expect(screen.getAllByText('18.0 s')).toHaveLength(2)
    expect(screen.getByText(/Then 12.0 s for 15 startup apps and services to settle, 30.0 s in all/)).toBeTruthy()
    expect(screen.getByText(/Firmware took 6.4 s at the last power-on/)).toBeTruthy()
    expect(screen.queryByText(/Fast startup is on/)).toBeNull()
    expect(screen.getAllByRole('listitem')).toHaveLength(2)
    expect(screen.getByText('WavesSysSvc Service Application')).toBeTruthy()
    expect(screen.getByText('App · slowed 2 of the last 2 boots')).toBeTruthy()
    expect(screen.getByText('+12.0 s')).toBeTruthy()

    await fireEvent.click(screen.getByRole('button', { name: /Manage startup apps/ }))
    expect(openUrl).toHaveBeenCalledWith('ms-settings:startupapps')
  })

  test('no boots recorded yet', async () => {
    boot = { boots: [], culprits: [], firmwareMs: null, fastStartup: false }
    render(Hardware)
    expect(await screen.findByText(/No full boots recorded yet/)).toBeTruthy()
    expect(screen.queryByText(/Firmware took/)).toBeNull()
  })

  test('explains that fast startup boots are not timed', async () => {
    boot = { ...BOOT, fastStartup: true }
    render(Hardware)
    expect(await screen.findByText(/Fast startup is on/)).toBeTruthy()
  })

  test('a boot log error leaves the drive card working', async () => {
    boot = new Error('Startup history needs administrator rights.')
    render(Hardware)
    expect(await screen.findByText('Startup history needs administrator rights.')).toBeTruthy()
    expect(screen.getByText('Micron 2300 NVMe 1TB')).toBeTruthy()
  })

  test('lists USB devices with the battery draw each added', async () => {
    const at = new Date('2026-10-09T09:30:00').getTime()
    usb = {
      devices: [
        { name: 'Pixel 8', arrivedMs: at, suspended: false, drawW: 4.2, speed: null, bottleneck: null },
        { name: 'MX Master 3', arrivedMs: at, suspended: true, drawW: 0, speed: null, bottleneck: null },
        { name: 'USB hub', arrivedMs: null, suspended: false, drawW: null, speed: null, bottleneck: null },
      ],
      builtIn: 3,
    }
    render(Hardware)
    expect(await screen.findByText('Pixel 8')).toBeTruthy()
    expect(screen.getByText('+4.2 W')).toBeTruthy()
    expect(screen.getByText('Under 0.1 W')).toBeTruthy()
    expect(screen.getByText('Not measured')).toBeTruthy()
    expect(screen.getByText('Plugged in Oct 9, 9:30 AM · Always on')).toBeTruthy()
    expect(screen.getByText('Plugged in Oct 9, 9:30 AM · Sleeps when idle')).toBeTruthy()
    expect(screen.getByText('Always on')).toBeTruthy()
    expect(screen.getByText(/3 built-in devices, like the camera, aren't listed/)).toBeTruthy()
  })

  test('USB devices show their link speed and what holds a USB 3 device back', async () => {
    const dev = { arrivedMs: null, suspended: false, drawW: null }
    usb = {
      devices: [
        { ...dev, name: 'Portable SSD', speed: 'high', bottleneck: 'cable' },
        { ...dev, name: 'Flash Drive', speed: 'high', bottleneck: 'port' },
        { ...dev, name: 'Dock', speed: 'superPlus', bottleneck: null },
        { ...dev, name: 'Mouse', speed: 'full', bottleneck: null },
      ],
      builtIn: 0,
    }
    render(Hardware)
    expect(await screen.findByText('Portable SSD')).toBeTruthy()
    expect(screen.getAllByText('480 Mbps · Always on')).toHaveLength(2)
    expect(screen.getByText('10 Gbps · Always on')).toBeTruthy()
    expect(screen.getByText('12 Mbps · Always on')).toBeTruthy()
    expect(screen.getByText('USB 3 device held to USB 2 speed. Try a USB 3 cable.')).toBeTruthy()
    expect(screen.getByText('USB 3 device held to USB 2 speed. Plug it into a USB 3 port.')).toBeTruthy()
  })

  test('no USB devices plugged in', async () => {
    usb = { devices: [], builtIn: 1 }
    render(Hardware)
    expect(await screen.findByText('Nothing is plugged in.')).toBeTruthy()
    expect(screen.getByText(/1 built-in device, like the camera, isn't listed/)).toBeTruthy()
  })

  test('the USB list keeps itself up to date', async () => {
    vi.useFakeTimers()
    try {
      render(Hardware)
      await vi.advanceTimersByTimeAsync(0)
      const usbCalls = () => invoke.mock.calls.filter(([c]) => c === 'get_usb').length
      expect(usbCalls()).toBe(1)
      await vi.advanceTimersByTimeAsync(5000)
      expect(usbCalls()).toBe(2)
    } finally {
      vi.useRealTimers()
    }
  })

  test('refresh rereads and a failed refresh keeps the old data', async () => {
    render(Hardware)
    const refresh = await screen.findByRole('button', { name: /Refresh/ })
    boot = new Error('Boom')
    await fireEvent.click(refresh)
    await screen.findByRole('button', { name: /Refresh/ })
    expect(invoke.mock.calls.filter(([c]) => c === 'get_boot')).toHaveLength(2)
    expect(screen.getByText(/last full restart/)).toBeTruthy()
    expect(toasts.items.map((t) => t.text)).toEqual(['Boom'])
  })
})
