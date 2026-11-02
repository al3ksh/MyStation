<script lang="ts">
  import type { IconName } from '../lib/icons'
  import Icon from './Icon.svelte'

  interface Option {
    value: string
    label: string
    icon?: IconName
  }

  interface Props {
    label: string
    options: Option[]
    /** The pressed option, or several when the group picks many (days). */
    value: string | null | string[]
    onpick: (value: string) => void
  }

  let { label, options, value, onpick }: Props = $props()

  const pressed = (v: string) => (Array.isArray(value) ? value.includes(v) : value === v)
</script>

<div class="chips" role="group" aria-label={label}>
  {#each options as o (o.value)}
    <button type="button" class="chip" aria-pressed={pressed(o.value)} onclick={() => onpick(o.value)}>
      {#if o.icon}<Icon name={o.icon} />{/if}
      {o.label}
    </button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    cursor: default;
  }

  .chip:hover {
    background: var(--surface-hover);
  }

  .chip[aria-pressed='true'] {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--surface-hover);
  }
</style>
