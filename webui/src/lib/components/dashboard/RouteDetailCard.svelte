<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import RouteCard from '../RouteCard.svelte';
  import type { RouteConfig } from '../../types/config';

  export let route: RouteConfig | null = null;
  export let routeIndex = 0;
  export let mode: 'view' | 'edit' = 'view';

  const dispatch = createEventDispatcher<{ close: void }>();
</script>

<article class="rounded-2xl border border-white/60 bg-white/90 p-4 shadow-panel backdrop-blur">
  <div class="mb-3 flex items-center justify-between">
    <h2 class="text-base font-bold text-slate-800">Route Detail</h2>
    {#if route}
      <button class="rounded-md border border-slate-300 bg-white px-2.5 py-1 text-xs font-semibold text-slate-700 hover:bg-slate-100" on:click={() => dispatch('close')}>
        Close
      </button>
    {/if}
  </div>

  {#if route}
    <RouteCard {route} {routeIndex} {mode} />
  {:else}
    <div class="rounded-xl border border-dashed border-slate-300 bg-slate-50 px-4 py-10 text-center text-sm text-slate-500">
      Select a route from the table, then click View or Edit.
    </div>
  {/if}
</article>
