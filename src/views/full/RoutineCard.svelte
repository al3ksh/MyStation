<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Switch from '../../components/Switch.svelte'
  import { actionChips, conditionChip, conditionIcon, isInstant, isManual } from '../../lib/routines'
  import type { Routine } from '../../lib/types'

  interface Props {
    routine: Routine
    running: boolean
    first: boolean
    last: boolean
    onedit: () => void
    ontoggle: () => void
    onmove: (by: -1 | 1) => void
    onstart: () => void
    onstop: () => void
  }

  let { routine, running, first, last, onedit, ontoggle, onmove, onstart, onstop }: Props = $props()

  const manual = $derived(isManual(routine))
  const undoes = $derived(routine.restore && !isInstant(routine))
</script>

<div class="card" class:running class:off={!routine.enabled} role="group" aria-label={routine.name}>
  <div class="top">
    <!-- The name and chips open the editor; the switch and tools sit beside, never inside. -->
    <button type="button" class="main" aria-label="Edit {routine.name}" onclick={onedit}>
      <span class="name">{routine.name}</span>
      <span class="chips">
        {#if manual}
          <span class="chip"><Icon name="play" />Manually</span>
        {/if}
        {#each routine.conditions as c (c.kind)}
          <span class="chip"><Icon name={conditionIcon(c.kind)} />{conditionChip(c)}</span>
        {/each}
        <span class="then"><Icon name="forward" /></span>
        {#each actionChips(routine.actions) as a (a.text)}
          <span class="chip"><Icon name={a.icon} />{a.text}</span>
        {/each}
        {#if routine.notify}
          <span class="chip only" title="Notifies"><Icon name="bell" /></span>
        {/if}
        {#if undoes}
          <span class="chip only" title={manual ? 'Undoes when it stops' : 'Undoes when it ends'}><Icon name="undo" /></span>
        {/if}
      </span>
    </button>
    <Switch checked={routine.enabled} label={routine.name} onchange={ontoggle} />
  </div>

  <div class="bottom">
    {#if running}
      <span class="status">Running</span>
      <button type="button" class="btn" aria-label="Stop {routine.name}" onclick={onstop}>
        <Icon name="stop" /> Stop
      </button>
    {:else if manual && routine.enabled}
      <button type="button" class="btn" aria-label="Start {routine.name}" onclick={onstart}>
        <Icon name="play" /> Start
      </button>
    {/if}
    <span class="tools">
      <button
        type="button"
        class="icon-btn"
        aria-label="Move {routine.name} up"
        disabled={first}
        onclick={() => onmove(-1)}><Icon name="up" /></button
      >
      <button
        type="button"
        class="icon-btn"
        aria-label="Move {routine.name} down"
        disabled={last}
        onclick={() => onmove(1)}><Icon name="down" /></button
      >
    </span>
  </div>
</div>

<style>
  .card {
    display: grid;
    gap: 8px;
    padding: 12px 12px 8px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-card);
  }

  .card.running {
    border-color: var(--accent);
  }

  .card.off .main {
    opacity: 0.6;
  }

  .top {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .main {
    display: grid;
    flex: 1;
    gap: 10px;
    min-width: 0;
    padding: 4px 0;
    font: inherit;
    color: inherit;
    text-align: left;
    background: none;
    border: none;
    cursor: default;
  }

  .name {
    font-size: 15px;
    font-weight: 600;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    font-size: 13px;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  .chip.only {
    padding: 0 9px;
  }

  .then {
    color: var(--text-2);
  }

  .bottom {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
  }

  .status {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }

  .tools {
    display: flex;
    gap: 2px;
    margin-left: auto;
    opacity: 0;
  }

  .card:hover .tools,
  .card:focus-within .tools {
    opacity: 1;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-ctl);
    cursor: default;
  }

  .btn:hover {
    background: var(--surface-raised);
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: inherit;
    background: none;
    border: none;
    border-radius: var(--r-ctl);
    cursor: default;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .icon-btn:disabled {
    opacity: 0.35;
  }
</style>
