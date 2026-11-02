<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { newId } from '../../lib/routines'
  import { errorText, toasts } from '../../lib/toasts.svelte'
  import type { Routine, RoutineOptions, RoutineStatus } from '../../lib/types'
  import RoutineCard from './RoutineCard.svelte'
  import RoutineEditor from './RoutineEditor.svelte'

  let routines = $state<Routine[] | null>(null)
  let running = $state<string[]>([])
  let options = $state<RoutineOptions>({ ssid: null, networks: [], bluetooth: [] })

  /** The routine in the editor; `index` is where it goes, `null` for a new one. */
  let editing = $state<{ routine: Routine; index: number | null } | null>(null)

  onMount(() => {
    api.getRoutines().then(
      (r) => (routines = r),
      (e) => toasts.push(errorText(e)),
    )
    api.routineStatus().then(
      (s) => (running = s.running),
      () => {},
    )
    const unlisten = listen<RoutineStatus>('routine-status', (e) => (running = e.payload.running))
    return () => void unlisten.then((u) => u())
  })

  // Every change saves at once, like Windows Settings; saves run in order.
  let saving = Promise.resolve()
  function save(next: Routine[]) {
    routines = next
    saving = saving.then(async () => {
      try {
        await api.setRoutines(next)
      } catch (e) {
        toasts.push(errorText(e))
        routines = await api.getRoutines().catch(() => routines)
      }
    })
  }

  function edit(change: (r: Routine[]) => void) {
    if (!routines) return
    const next = $state.snapshot(routines) as Routine[]
    change(next)
    save(next)
  }

  function open(index: number | null) {
    const r = index === null ? null : routines?.[index]
    editing = {
      index: r ? index : null,
      routine: r
        ? ($state.snapshot(r) as Routine)
        : {
            id: newId(),
            name: '',
            enabled: true,
            conditions: [],
            actions: { thermal: null, battery: null, powerMode: null },
            notify: false,
            restore: false,
          },
    }
    api.routineOptions().then(
      (o) => (options = o),
      () => {},
    )
  }

  function finish(r: Routine) {
    const index = editing?.index ?? null
    edit((list) => {
      if (index === null) list.push(r)
      else list[index] = r
    })
    editing = null
  }

  function remove() {
    const index = editing?.index ?? null
    if (index !== null) edit((list) => list.splice(index, 1))
    editing = null
  }

  function act(call: Promise<void>) {
    call.catch((e) => toasts.push(errorText(e)))
  }
</script>

{#if editing}
  <RoutineEditor
    routine={editing.routine}
    isNew={editing.index === null}
    {options}
    onsave={finish}
    oncancel={() => (editing = null)}
    ondelete={remove}
  />
{:else if routines}
  <div class="page">
    <p class="secondary intro">
      A routine runs while all its conditions hold. It switches the settings when it starts and, with Undo, puts them
      back when it ends. A change you make yourself holds until a routine starts again. When two start at once, the
      higher one wins.
    </p>

    <div class="grid">
      {#each routines as r, i (r.id)}
        <RoutineCard
          routine={r}
          running={running.includes(r.id)}
          first={i === 0}
          last={i === routines.length - 1}
          onedit={() => open(i)}
          ontoggle={() => edit((list) => (list[i].enabled = !list[i].enabled))}
          onmove={(by) => edit((list) => list.splice(i + by, 0, ...list.splice(i, 1)))}
          onstart={() => act(api.startRoutine(r.id))}
          onstop={() => act(api.stopRoutine(r.id))}
        />
      {/each}
      <button type="button" class="new" onclick={() => open(null)}>
        <Icon name="add" /> New routine
      </button>
    </div>

    {#if !routines.length}
      <p class="secondary empty">
        No routines yet. Make one to switch modes when you join a network, plug in, dock, at a set time, or by hand.
      </p>
    {/if}
  </div>
{/if}

<style>
  .page {
    display: grid;
    gap: 12px;
    max-width: 880px;
  }

  .intro,
  .empty {
    margin: 0;
    font-size: 13px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 12px;
  }

  .new {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 112px;
    font: inherit;
    color: var(--text-2);
    background: none;
    border: 1px dashed var(--border-strong);
    border-radius: var(--r-card);
    cursor: default;
  }

  .new:hover {
    color: inherit;
    background: var(--surface-hover);
  }
</style>
