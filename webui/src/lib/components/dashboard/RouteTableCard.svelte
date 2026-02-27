<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RouteHealthItem } from '../../api/admin';
  import type { RouteConfig } from '../../types/config';

  interface RouteRow {
    route: RouteConfig;
    routeIndex: number;
  }

  export let routeQuery = '';
  export let rows: RouteRow[] = [];
  export let selectedRouteIndex: number | null = null;
  export let routeHealthByIndex: Record<number, RouteHealthItem> = {};
  export let healthLoading = false;
  export let healthError = '';

  const dispatch = createEventDispatcher<{
    addRoute: void;
    refreshHealth: void;
    search: string;
    view: number;
    edit: number;
    delete: number;
  }>();

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;

  const getRouteHealth = (row: RouteRow): RouteHealthItem | null =>
    routeHealthByIndex[row.routeIndex] ?? null;

  const healthTooltip = (health: RouteHealthItem): string =>
    health.upstreams
      .map((upstream) =>
        upstream.healthy
          ? `${upstream.addr}: UP (${upstream.latency_ms ?? 0}ms)`
          : `${upstream.addr}: DOWN (${upstream.error ?? 'unreachable'})`
      )
      .join('\n');

  const routeHealthTooltip = (row: RouteRow): string => {
    const health = getRouteHealth(row);
    return health ? healthTooltip(health) : '';
  };

  const routeHealthLabel = (row: RouteRow): string => {
    const health = getRouteHealth(row);
    if (!health) {
      return 'UNKNOWN';
    }
    if (health.reachable_upstreams === 0) {
      return 'DOWN';
    }
    if (health.reachable_upstreams < health.total_upstreams) {
      return 'DEGRADED';
    }
    return health.healthy ? 'UP' : 'DEGRADED';
  };

  const routeHealthLabelClass = (row: RouteRow): string => {
    const label = routeHealthLabel(row);
    if (label === 'UP') {
      return 'rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-200';
    }
    if (label === 'DEGRADED') {
      return 'rounded-full border border-amber-400/40 bg-amber-500/10 px-2 py-0.5 text-xs font-semibold text-amber-200';
    }
    if (label === 'DOWN') {
      return 'rounded-full border border-rose-400/40 bg-rose-500/10 px-2 py-0.5 text-xs font-semibold text-rose-200';
    }
    return 'rounded-full border border-slate-500 bg-slate-800 px-2 py-0.5 text-xs font-semibold text-slate-300';
  };

  const upstreamHealthClass = (healthy: boolean): string =>
    healthy
      ? 'rounded border border-emerald-400/40 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-200'
      : 'rounded border border-rose-400/40 bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-rose-200';

  const upstreamHealthText = (row: RouteRow): string[] => {
    const health = getRouteHealth(row);
    if (!health) {
      return [];
    }
    return health.upstreams.map((upstream, idx) =>
      upstream.healthy ? `U${idx + 1}:UP` : `U${idx + 1}:DOWN`
    );
  };
</script>

<article class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-4 backdrop-blur">
  <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
    <h2 class="text-base font-bold text-slate-100">Routes</h2>
    <div class="flex w-full items-center gap-2 md:w-auto">
      <input
        class="w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 md:w-72"
        placeholder="Search route name / host / path / strategy"
        value={routeQuery}
        on:input={(e) => dispatch('search', inputValue(e))}
      />
      <button
        class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-2 text-xs font-semibold text-cyan-200 hover:bg-cyan-500/20"
        on:click={() => dispatch('addRoute')}
      >
        Add Route
      </button>
      <button
        class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-2 text-xs font-semibold text-emerald-200 hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={healthLoading}
        on:click={() => dispatch('refreshHealth')}
      >
        {healthLoading ? 'Checking...' : 'Check Health'}
      </button>
    </div>
  </div>

  {#if healthError}
    <div class="mb-3 rounded-lg border border-rose-400/40 bg-rose-500/10 px-3 py-2 text-xs font-medium text-rose-200">
      Health check failed: {healthError}
    </div>
  {/if}

  <div class="overflow-hidden rounded-xl border border-slate-700 bg-slate-950/70">
    <div class="max-h-[46vh] overflow-auto">
      <table class="min-w-full divide-y divide-slate-800 text-sm">
        <thead class="sticky top-0 z-10 bg-slate-900 text-slate-300">
          <tr>
            <th class="px-4 py-3 text-left font-semibold">Route</th>
            <th class="px-4 py-3 text-left font-semibold">Host</th>
            <th class="px-4 py-3 text-left font-semibold">Path</th>
            <th class="px-4 py-3 text-left font-semibold">LB</th>
            <th class="px-4 py-3 text-left font-semibold">Health</th>
            <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
            <th class="px-4 py-3 text-left font-semibold">Actions</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-slate-800">
          {#if rows.length === 0}
            <tr>
              <td class="px-4 py-6 text-center text-sm text-slate-400" colspan="7">No routes found.</td>
            </tr>
          {:else}
            {#each rows as row}
              <tr class={selectedRouteIndex === row.routeIndex ? 'bg-cyan-500/10' : 'hover:bg-slate-900/70'}>
                <td class="px-4 py-3 font-medium text-slate-100">
                  {row.route.name}
                  {#if row.route.is_default}
                    <span class="ml-2 rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-200">default</span>
                  {/if}
                </td>
                <td class="px-4 py-3 text-slate-300">{row.route.host || '-'}</td>
                <td class="px-4 py-3 text-slate-300">{row.route.path_prefix}</td>
                <td class="px-4 py-3 text-slate-300">{row.route.lb}</td>
                <td class="px-4 py-3">
                  <span class={routeHealthLabelClass(row)} title={routeHealthTooltip(row)}>{routeHealthLabel(row)}</span>
                </td>
                <td class="px-4 py-3 text-slate-300">
                  <div class="flex flex-wrap gap-1.5">
                    {#if upstreamHealthText(row).length === 0}
                      <span class="text-xs text-slate-500">n/a</span>
                    {:else}
                      {#each getRouteHealth(row)?.upstreams ?? [] as upstream, idx}
                        <span class={upstreamHealthClass(upstream.healthy)} title={upstream.error ?? ''}>
                          U{idx + 1}:{upstream.healthy ? 'UP' : 'DOWN'}
                        </span>
                      {/each}
                    {/if}
                  </div>
                </td>
                <td class="px-4 py-3">
                  <div class="flex flex-wrap gap-2">
                    <button
                      class="rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-800"
                      on:click={() => dispatch('view', row.routeIndex)}
                    >
                      View
                    </button>
                    <button
                      class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-2.5 py-1 text-xs font-semibold text-cyan-200 hover:bg-cyan-500/20"
                      on:click={() => dispatch('edit', row.routeIndex)}
                    >
                      Edit
                    </button>
                    <button
                      class="rounded-md border border-rose-400/40 bg-rose-500/10 px-2.5 py-1 text-xs font-semibold text-rose-200 hover:bg-rose-500/20"
                      on:click={() => dispatch('delete', row.routeIndex)}
                    >
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  </div>
</article>
