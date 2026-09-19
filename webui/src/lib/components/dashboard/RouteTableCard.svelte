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
      return 'rounded-full border border-success/40 bg-success/10 px-2 py-0.5 text-xs font-semibold text-success';
    }
    if (label === 'DEGRADED') {
      return 'rounded-full border border-warning/40 bg-warning/10 px-2 py-0.5 text-xs font-semibold text-warning';
    }
    if (label === 'DOWN') {
      return 'rounded-full border border-destructive/40 bg-destructive/10 px-2 py-0.5 text-xs font-semibold text-destructive';
    }
    return 'rounded-full border border-border bg-muted px-2 py-0.5 text-xs font-semibold text-foreground/80';
  };

  const upstreamHealthClass = (healthy: boolean): string =>
    healthy
      ? 'rounded border border-success/40 bg-success/10 px-1.5 py-0.5 text-[10px] font-semibold text-success'
      : 'rounded border border-destructive/40 bg-destructive/10 px-1.5 py-0.5 text-[10px] font-semibold text-destructive';

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

<article class="rounded-2xl border border-border/80 bg-card/80 p-4 backdrop-blur">
  <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
    <h2 class="text-base font-bold text-foreground">Routes</h2>
    <div class="flex w-full items-center gap-2 md:w-auto">
      <input
        class="w-full rounded-lg border border-border bg-card px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground md:w-72"
        placeholder="Search route name / host / path / strategy"
        value={routeQuery}
        on:input={(e) => dispatch('search', inputValue(e))}
      />
      <button
        class="rounded-md border border-primary/40 bg-primary/10 px-3 py-2 text-xs font-semibold text-primary hover:bg-primary/20"
        on:click={() => dispatch('addRoute')}
      >
        Add Route
      </button>
      <button
        class="rounded-md border border-success/40 bg-success/10 px-3 py-2 text-xs font-semibold text-success hover:bg-success/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={healthLoading}
        on:click={() => dispatch('refreshHealth')}
      >
        {healthLoading ? 'Checking...' : 'Check Health'}
      </button>
    </div>
  </div>

  {#if healthError}
    <div class="mb-3 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs font-medium text-destructive">
      Health check failed: {healthError}
    </div>
  {/if}

  <div class="overflow-hidden rounded-xl border border-border bg-background/70">
    <div class="max-h-[46vh] overflow-auto">
      <table class="min-w-full divide-y divide-border text-sm">
        <thead class="sticky top-0 z-10 bg-card text-foreground/80">
          <tr>
            <th class="px-4 py-3 text-left font-semibold">Route</th>
            <th class="px-4 py-3 text-left font-semibold">Host</th>
            <th class="px-4 py-3 text-left font-semibold">Path</th>
            <th class="px-4 py-3 text-left font-semibold">Service</th>
            <th class="px-4 py-3 text-left font-semibold">Health</th>
            <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
            <th class="px-4 py-3 text-left font-semibold">Actions</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-border">
          {#if rows.length === 0}
            <tr>
              <td class="px-4 py-6 text-center text-sm text-muted-foreground" colspan="7">No routes found.</td>
            </tr>
          {:else}
            {#each rows as row}
              <tr class={selectedRouteIndex === row.routeIndex ? 'bg-primary/10' : 'hover:bg-card/70'}>
                <td class="px-4 py-3 font-medium text-foreground">
                  {row.route.name}
                  {#if row.route.is_default}
                    <span class="ml-2 rounded-full border border-success/40 bg-success/10 px-2 py-0.5 text-xs font-semibold text-success">default</span>
                  {/if}
                </td>
                <td class="px-4 py-3 text-foreground/80">{row.route.host || '-'}</td>
                <td class="px-4 py-3 text-foreground/80">{row.route.path_prefix}</td>
                <td class="px-4 py-3 text-foreground/80">{row.route.service || '-'}</td>
                <td class="px-4 py-3">
                  <span class={routeHealthLabelClass(row)} title={routeHealthTooltip(row)}>{routeHealthLabel(row)}</span>
                </td>
                <td class="px-4 py-3 text-foreground/80">
                  <div class="flex flex-wrap gap-1.5">
                    {#if upstreamHealthText(row).length === 0}
                      <span class="text-xs text-muted-foreground">n/a</span>
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
                      class="rounded-md border border-border bg-card px-2.5 py-1 text-xs font-semibold text-foreground hover:bg-muted"
                      on:click={() => dispatch('view', row.routeIndex)}
                    >
                      View
                    </button>
                    <button
                      class="rounded-md border border-primary/40 bg-primary/10 px-2.5 py-1 text-xs font-semibold text-primary hover:bg-primary/20"
                      on:click={() => dispatch('edit', row.routeIndex)}
                    >
                      Edit
                    </button>
                    <button
                      class="rounded-md border border-destructive/40 bg-destructive/10 px-2.5 py-1 text-xs font-semibold text-destructive hover:bg-destructive/20"
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
