<script lang="ts">
  import Flyout from './views/Flyout.svelte'
  import FullWindow from './views/FullWindow.svelte'
  import Records from './views/Records.svelte'

  let { label }: { label: string } = $props()
</script>

{#if label === 'flyout'}
  <Flyout />
{:else if label === 'records'}
  <Records />
{:else}
  <FullWindow />
{/if}
