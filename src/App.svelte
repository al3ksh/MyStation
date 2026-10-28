<script lang="ts">
  import Flyout from './views/Flyout.svelte'
  import FullWindow from './views/FullWindow.svelte'
  import Monitor from './views/Monitor.svelte'

  let { label }: { label: string } = $props()
</script>

{#if label === 'flyout'}
  <Flyout />
{:else if label === 'monitor'}
  <Monitor />
{:else}
  <FullWindow />
{/if}
