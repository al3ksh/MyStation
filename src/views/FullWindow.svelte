<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte'
  import Banner from '../components/Banner.svelte'
  import Icon from '../components/Icon.svelte'
  import Toasts from '../components/Toasts.svelte'
  import { api } from '../lib/api'
  import { bannersFor } from '../lib/banners'
  import { appName, FALLBACK_NAME } from '../lib/brand'
  import { history } from '../lib/history.svelte'
  import type { IconName } from '../lib/icons'
  import { live, startWindow } from '../lib/telemetry.svelte'
  import { errorText, toasts } from '../lib/toasts.svelte'
  import type { DeviceInfo } from '../lib/types'
  import Automation from './full/Automation.svelte'
  import Bios from './full/Bios.svelte'
  import Hardware from './full/Hardware.svelte'
  import Spec from './full/Spec.svelte'
  import Battery from './full/Battery.svelte'
  import Overview from './full/Overview.svelte'
  import Performance from './full/Performance.svelte'
  import Power from './full/Power.svelte'
  import Sensors from './full/Sensors.svelte'
  import Settings from './full/Settings.svelte'

  type Section = 'Overview' | 'Battery' | 'Power' | 'Performance' | 'Sensors' | 'Automation' | 'Hardware' | 'Spec' | 'BIOS' | 'Settings'
  const SECTIONS: { name: Section; icon: IconName }[] = [
    { name: 'Overview', icon: 'overview' },
    { name: 'Battery', icon: 'battery' },
    { name: 'Power', icon: 'lightning' },
    { name: 'Performance', icon: 'gauge' },
    { name: 'Sensors', icon: 'chart' },
    { name: 'Automation', icon: 'automation' },
    { name: 'Hardware', icon: 'drive' },
    { name: 'Spec', icon: 'laptop' },
    { name: 'BIOS', icon: 'bios' },
  ]

  let section = $state<Section>('Overview')
  let device = $state<DeviceInfo | null>(null)
  let name = $state(FALLBACK_NAME)

  onMount(() => {
    const loaded = Promise.all([
      history.load().catch((e) => toasts.push(errorText(e))),
      // Identity is cosmetic here: a failure leaves the card generic.
      api.getDeviceInfo().then(
        (d) => (device = d),
        () => {},
      ),
      appName().then((n) => (name = n)),
    ])
    // Charts and the device card are part of the first paint, not something that pops in after it.
    void startWindow(
      (e) => toasts.push(errorText(e)),
      async () => {
        await loaded
        await tick()
      },
    )
  })
  onDestroy(() => {
    live.stop()
    history.clear()
  })

  $effect(() => {
    const t = live.telemetry
    // Only a new tick re-runs this; append reads the samples it replaces.
    if (t) untrack(() => history.append(t))
  })

  const app = $derived(live.app)
  const banners = $derived(app ? bannersFor(app) : [])
</script>

{#snippet item(name: Section, icon: IconName)}
  <button
    type="button"
    class="item"
    class:active={section === name}
    aria-current={section === name ? 'page' : undefined}
    onclick={() => (section = name)}
  >
    <Icon name={icon} />
    <span>{name}</span>
  </button>
{/snippet}

<div class="window">
  <nav>
    <div class="device">
      <span class="avatar"><Icon name="laptop" size={20} /></span>
      <div>
        <div class="model">{device?.model ?? 'Dell Precision'}</div>
        {#if device?.serviceTag}
          <div class="secondary tag">Service tag <span class="num">{device.serviceTag}</span></div>
        {/if}
      </div>
    </div>

    {#each SECTIONS as s (s.name)}
      {@render item(s.name, s.icon)}
    {/each}

    <div class="spacer"></div>
    {@render item('Settings', 'settings')}
  </nav>

  <main>
    <h1>{section}</h1>
    {#if banners.length}
      <div class="banners">
        {#each banners as id (id)}
          <Banner {id} />
        {/each}
      </div>
    {/if}
    {#if section === 'Overview'}
      <Overview samples={history.samples} />
    {:else if section === 'Battery'}
      <Battery samples={history.samples} />
    {:else if section === 'Power'}
      <Power />
    {:else if section === 'Performance'}
      <Performance samples={history.samples} />
    {:else if section === 'Sensors'}
      <Sensors samples={history.samples} />
    {:else if section === 'Automation'}
      <Automation />
    {:else if section === 'Hardware'}
      <Hardware />
    {:else if section === 'Spec'}
      <Spec />
    {:else if section === 'BIOS'}
      <Bios />
    {:else}
      <Settings {device} {name} />
    {/if}

    <Toasts floating />
  </main>
</div>

<style>
  .window {
    height: 100%;
    display: grid;
    grid-template-columns: 248px 1fr;
    /* One viewport-high row: only <main> scrolls, never the window. */
    grid-template-rows: minmax(0, 1fr);
    overflow: hidden;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px 12px;
    overflow-y: auto;
  }

  .device {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px 18px;
  }

  .avatar {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--surface-hover);
    color: var(--accent);
  }

  .model {
    font-weight: 600;
  }

  .tag {
    font-size: 12px;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 36px;
    padding: 0 12px;
    text-align: left;
    background: none;
    border: none;
    border-radius: var(--r-ctl);
    cursor: default;
    transition: background 120ms var(--ease);
  }

  .item:hover {
    background: var(--surface);
  }

  .item.active {
    background: var(--surface-hover);
  }

  .item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  main {
    min-width: 0;
    min-height: 0;
    overscroll-behavior: contain;
    overflow-y: auto;
    padding: 20px 32px 32px;
    background: var(--layer);
    border-top: 1px solid var(--border);
    border-left: 1px solid var(--border);
    border-top-left-radius: 8px;
  }

  h1 {
    margin: 0 0 20px;
    font-size: 28px;
    font-weight: 600;
  }

  .banners {
    display: grid;
    gap: 8px;
    max-width: 880px;
    margin-bottom: 12px;
  }
</style>
