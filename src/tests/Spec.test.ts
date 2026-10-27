import { render, screen } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { Spec as SpecData } from '../lib/types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))

const { default: Spec, forget } = await import('../views/full/Spec.svelte')

function spec(over: Partial<SpecData> = {}): SpecData {
  return {
    model: 'Precision 5560',
    biosVersion: '1.30.0',
    biosDate: '08/12/2026',
    cpu: {
      name: '11th Gen Intel(R) Core(TM) i7-11850H @ 2.50GHz',
      cores: 8,
      threads: 16,
      l2Kb: 10240,
      l3Kb: 24576,
      features: ['AVX2', 'AVX-512', 'AES-NI', 'SHA'],
      virtualization: true,
    },
    memory: {
      maxGb: 64,
      slots: 2,
      modules: [
        { slot: 'DIMM A', sizeMb: 16384, kind: 'DDR4', speedMts: 3200, configuredMts: 3200, maker: 'Kingston', part: '9905744-108.A00G' },
        { slot: 'DIMM B', sizeMb: 16384, kind: 'DDR4', speedMts: 3200, configuredMts: 2933, maker: 'Kingston', part: '9905744-108.A00G' },
      ],
    },
    graphics: [
      { name: 'NVIDIA T1200 Laptop GPU', driver: '32.0.15.9716', vramMb: 4096, link: { generation: 4, lanes: 16 } },
      { name: 'Intel(R) UHD Graphics', driver: '32.0.101.7088', vramMb: null, link: null },
    ],
    storage: [{ name: 'Micron 2300 NVMe 512GB', sizeGb: 512, link: { generation: 3, lanes: 4 } }],
    panel: { maker: 'LG Display', width: 1920, height: 1200, maxHz: 65, diagonalIn: 15.7 },
    wifi: 'Intel(R) Wi-Fi 6 AX201 160MHz',
    bluetooth: '5.2',
    thunderbolt: 'Thunderbolt',
    tpm: '2.0',
    secureBoot: true,
    fingerprint: 'Goodix MOC Fingerprint',
    ...over,
  }
}

function serve(s: SpecData | string) {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd !== 'get_spec') throw `unexpected ${cmd}`
    if (typeof s === 'string') throw s
    return s
  })
}

describe('Spec', () => {
  beforeEach(() => {
    invoke.mockReset()
    forget()
  })

  test('processor: cores, cache, instructions and virtualization', async () => {
    serve(spec())
    render(Spec)
    expect(await screen.findByText('11th Gen Intel(R) Core(TM) i7-11850H @ 2.50GHz')).toBeTruthy()
    expect(screen.getByText('8 cores, 16 threads')).toBeTruthy()
    expect(screen.getByText('L2 10 MB · L3 24 MB')).toBeTruthy()
    expect(screen.getByText('AVX2 · AVX-512 · AES-NI · SHA')).toBeTruthy()
    expect(screen.getByText('Enabled')).toBeTruthy()
  })

  test('memory: installed against the maximum, and a module running below its rating', async () => {
    serve(spec())
    render(Spec)
    expect(await screen.findByText('32 GB of 64 GB')).toBeTruthy()
    expect(screen.getByText('2 of 2 used')).toBeTruthy()
    expect(screen.getByText('16 GB DDR4-3200 · Kingston 9905744-108.A00G')).toBeTruthy()
    expect(screen.getByText('16 GB DDR4-3200, runs at 2933 · Kingston 9905744-108.A00G')).toBeTruthy()
  })

  test('graphics and storage show their PCIe links and sizes', async () => {
    serve(spec())
    render(Spec)
    expect(await screen.findByText('4 GB · PCIe 4.0 x16 · Driver 32.0.15.9716')).toBeTruthy()
    expect(screen.getByText('Driver 32.0.101.7088')).toBeTruthy()
    expect(screen.getByText('512 GB · PCIe 3.0 x4')).toBeTruthy()
  })

  test('display, connectivity and security', async () => {
    serve(spec())
    render(Spec)
    expect(await screen.findByText('15.7″ · 1920 × 1200 · up to 65 Hz')).toBeTruthy()
    expect(screen.getByText('LG Display')).toBeTruthy()
    expect(screen.getByText('Version 5.2')).toBeTruthy()
    expect(screen.getByText('2.0')).toBeTruthy()
    expect(screen.getByText('On')).toBeTruthy()
  })

  test('what the laptop lacks or did not report is left out', async () => {
    serve(spec({ thunderbolt: null, fingerprint: null, panel: null, cpu: null }))
    render(Spec)
    await screen.findByText('Micron 2300 NVMe 512GB')
    expect(screen.queryByText('Thunderbolt')).toBeNull()
    expect(screen.queryByText('Fingerprint reader')).toBeNull()
    expect(screen.queryByRole('heading', { name: 'Display' })).toBeNull()
    expect(screen.queryByRole('heading', { name: 'Processor' })).toBeNull()
  })

  test('a 1 TB drive reads as terabytes', async () => {
    serve(spec({ storage: [{ name: 'Samsung 990', sizeGb: 1024, link: null }] }))
    render(Spec)
    expect(await screen.findByText('1 TB')).toBeTruthy()
  })

  test('a failed read shows the error', async () => {
    serve('Access denied')
    render(Spec)
    expect(await screen.findByText('Access denied')).toBeTruthy()
  })

  test('the spec is read once, then kept across visits', async () => {
    serve(spec())
    const first = render(Spec)
    await screen.findByText('Micron 2300 NVMe 512GB')
    first.unmount()
    render(Spec)
    expect(screen.getByText('Micron 2300 NVMe 512GB')).toBeTruthy()
    expect(invoke).toHaveBeenCalledTimes(1)
  })
})
