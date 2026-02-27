<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import RouteCard from '../RouteCard.svelte';
  import type { RouteConfig } from '../../types/config';

  export let route: RouteConfig | null = null;
  export let routeIndex = 0;
  export let mode: 'view' | 'edit' = 'view';

  const dispatch = createEventDispatcher<{ close: void }>();
</script>

<article class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-4 backdrop-blur">
  <div class="mb-3 flex items-center justify-between">
    <h2 class="text-base font-bold text-slate-100">Route Detail</h2>
    {#if route}
      <button class="rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-800" on:click={() => dispatch('close')}>
        Close
      </button>
    {/if}
  </div>

  {#if route}
    <RouteCard {route} {routeIndex} {mode} />
  {:else}
    <div class="rounded-xl border border-dashed border-slate-600 bg-slate-950/60 px-4 py-10 text-center text-sm text-slate-400">
      Select a route from the table, then click View or Edit.
    </div>
  {/if}
</article>
