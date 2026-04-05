<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import StatsCard from '../dashboard/StatsCard.svelte';
  import type { PrxConfig, RouteConfig } from '../../types/config';
  import type { RouteHealthResponse, RouteHealthItem } from '../../api/admin';
  import type { NavPage } from '../../stores/navigation';

  export let config: PrxConfig;
  export let routeHealth: RouteHealthResponse | null = null;
  export let healthLoading: boolean = false;
  export let healthError: string = '';

  const dispatch = createEventDispatcher<{
    navigate: NavPage;
    addRoute: void;
    refreshHealth: void;
    exportJson: void;
  }>();

  // ---------------------------------------------------------------------------
  // Computed values
  // ---------------------------------------------------------------------------

  $: routes = config.routes ?? [];

  $: totalRoutes = routes.length;

  $: totalUpstreams = routes.reduce(
    (sum, route) => sum + (route.upstreams?.length ?? 0),
    0
  );

  $: tlsEnabledCount = routes.reduce(
    (sum, route) =>
      sum + (route.upstreams?.filter((u) => u.tls === true).length ?? 0),
    0
  );

  $: circuitBreakerCount = routes.filter(
    (route) => route.circuit_breaker?.enabled === true
  ).length;

  // Health summary
  $: healthRoutes = routeHealth?.routes ?? [];

  $: healthyCount = healthRoutes.filter(
    (r) => r.healthy && r.reachable_upstreams === r.total_upstreams
  ).length;

  $: degradedCount = healthRoutes.filter(
    (r) => r.reachable_upstreams > 0 && r.reachable_upstreams < r.total_upstreams
  ).length;

  $: downCount = healthRoutes.filter(
    (r) => r.reachable_upstreams === 0
  ).length;

  $: unknownCount = Math.max(0, totalRoutes - healthRoutes.length);

  // Recent routes (first 5)
  $: recentRoutes = routes.slice(0, 5);

  $: hasMoreRoutes = routes.length > 5;

  // Health data by index for quick lookup
  $: healthByIndex = (() => {
    const map: Record<number, RouteHealthItem> = {};
    for (const item of healthRoutes) {
      map[item.route_index] = item;
    }
    return map;
  })();

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  const getRouteHealth = (routeIndex: number): RouteHealthItem | null =>
    healthByIndex[routeIndex] ?? null;

  const routeHealthStatus = (routeIndex: number): 'healthy' | 'degraded' | 'down' | 'unknown' => {
    const health = getRouteHealth(routeIndex);
    if (!health) return 'unknown';
    if (health.reachable_upstreams === 0) return 'down';
    if (health.reachable_upstreams < health.total_upstreams) return 'degraded';
    return health.healthy ? 'healthy' : 'degraded';
  };

  const statusDotClass = (status: string): string => {
    switch (status) {
      case 'healthy':
        return 'bg-emerald-400';
      case 'degraded':
        return 'bg-amber-400';
      case 'down':
        return 'bg-rose-400';
      default:
        return 'bg-slate-500';
    }
  };

  const statusBadgeClass = (status: string): string => {
    switch (status) {
      case 'healthy':
        return 'rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-200';
      case 'degraded':
        return 'rounded-full border border-amber-400/40 bg-amber-500/10 px-2 py-0.5 text-xs font-semibold text-amber-200';
      case 'down':
        return 'rounded-full border border-rose-400/40 bg-rose-500/10 px-2 py-0.5 text-xs font-semibold text-rose-200';
      default:
        return 'rounded-full border border-slate-500 bg-slate-800 px-2 py-0.5 text-xs font-semibold text-slate-300';
    }
  };

  const formatLbStrategy = (lb: string): string => {
    switch (lb) {
      case 'round_robin':
        return 'Round Robin';
      case 'random':
        return 'Random';
      case 'hash':
        return 'Hash';
      default:
        return lb;
    }
  };

  const healthCheckedAt = (): string => {
    if (!routeHealth?.checked_at_epoch_ms) return '';
    const date = new Date(routeHealth.checked_at_epoch_ms);
    return date.toLocaleTimeString();
  };
</script>

<AppLayout title="Dashboard" subtitle="Overview of your proxy configuration">
  <svelte:fragment slot="header-actions">
    <button
      class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
      on:click={() => dispatch('exportJson')}
    >
      Export JSON
    </button>
    <button
      class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
      on:click={() => dispatch('refreshHealth')}
    >
      {healthLoading ? 'Checking...' : 'Refresh Health'}
    </button>
  </svelte:fragment>

  <div class="space-y-6 p-6">
    <!-- Stats Cards Row -->
    <section>
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-slate-400">
        Overview
      </h2>
      <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
        <StatsCard
          icon="⇌"
          label="Total Routes"
          value={totalRoutes}
          color="cyan"
        />
        <StatsCard
          icon="◉"
          label="Active Upstreams"
          value={totalUpstreams}
          color="emerald"
        />
        <StatsCard
          icon="🔒"
          label="TLS Enabled"
          value={tlsEnabledCount}
          color="violet"
        />
        <StatsCard
          icon="⚡"
          label="Circuit Breakers"
          value={circuitBreakerCount}
          color="amber"
        />
      </div>
    </section>

    <!-- Health Overview Section -->
    <section>
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-slate-400">
        Health Status
      </h2>
      <div class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-5 backdrop-blur">
        {#if healthError}
          <div class="mb-4 rounded-lg border border-rose-400/40 bg-rose-500/10 px-4 py-3 text-sm font-medium text-rose-200">
            Health check failed: {healthError}
          </div>
        {/if}

        <!-- Summary Stats -->
        <div class="mb-4 flex flex-wrap items-center gap-6">
          <div class="flex items-center gap-2">
            <span class="h-2.5 w-2.5 rounded-full bg-emerald-400" />
            <span class="text-sm font-medium text-slate-200">
              {healthyCount} Healthy
            </span>
          </div>
          <div class="flex items-center gap-2">
            <span class="h-2.5 w-2.5 rounded-full bg-amber-400" />
            <span class="text-sm font-medium text-slate-200">
              {degradedCount} Degraded
            </span>
          </div>
          <div class="flex items-center gap-2">
            <span class="h-2.5 w-2.5 rounded-full bg-rose-400" />
            <span class="text-sm font-medium text-slate-200">
              {downCount} Down
            </span>
          </div>
          {#if unknownCount > 0}
            <div class="flex items-center gap-2">
              <span class="h-2.5 w-2.5 rounded-full bg-slate-500" />
              <span class="text-sm font-medium text-slate-200">
                {unknownCount} Unknown
              </span>
            </div>
          {/if}
        </div>

        <!-- Route Health Dots -->
        <div class="flex flex-wrap items-center gap-2">
          {#each routes as route, idx}
            <div
              class="group relative"
              title="{route.name}: {routeHealthStatus(idx)}"
            >
              <span class="h-3 w-3 rounded-full {statusDotClass(routeHealthStatus(idx))} transition-transform hover:scale-125" />
              <!-- Tooltip on hover -->
              <div class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-2 -translate-x-1/2 whitespace-nowrap rounded-lg border border-slate-600 bg-slate-800 px-2 py-1 text-xs text-slate-200 opacity-0 shadow-lg transition-opacity group-hover:opacity-100">
                {route.name}
                <div class="mt-0.5 text-slate-400">
                  {#if routeHealthStatus(idx) === 'healthy'}
                    All upstreams reachable
                  {:else if routeHealthStatus(idx) === 'degraded'}
                    Partial upstream failure
                  {:else if routeHealthStatus(idx) === 'down'}
                    No upstreams reachable
                  {:else}
                    Not yet checked
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        </div>

        {#if healthCheckedAt()}
          <p class="mt-3 text-xs text-slate-500">
            Last checked: {healthCheckedAt()}
          </p>
        {/if}
      </div>
    </section>

    <!-- Quick Actions Row -->
    <section>
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-slate-400">
        Quick Actions
      </h2>
      <div class="flex flex-wrap gap-3">
        <button
          class="inline-flex items-center gap-2 rounded-xl border border-cyan-400/40 bg-cyan-500/10 px-4 py-2.5 text-sm font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
          on:click={() => dispatch('addRoute')}
        >
          <span class="text-base">+</span>
          New Route
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-xl border border-emerald-400/40 bg-emerald-500/10 px-4 py-2.5 text-sm font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
          on:click={() => dispatch('exportJson')}
        >
          <span class="text-base">↓</span>
          Export Config
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-xl border border-slate-600 bg-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200 transition-colors hover:bg-slate-700"
          on:click={() => dispatch('navigate', 'settings')}
        >
          <span class="text-base">⟨/⟩</span>
          View TOML
        </button>
      </div>
    </section>

    <!-- Recent Routes Preview -->
    <section>
      <div class="mb-3 flex items-center justify-between">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-slate-400">
          Recent Routes
        </h2>
        {#if hasMoreRoutes}
          <button
            class="text-sm font-medium text-cyan-400 transition-colors hover:text-cyan-300"
            on:click={() => dispatch('navigate', 'routes')}
          >
            View All Routes →
          </button>
        {/if}
      </div>

      <div class="rounded-2xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
        <div class="overflow-hidden rounded-xl border border-slate-700 bg-slate-950/70">
          <table class="min-w-full divide-y divide-slate-800 text-sm">
            <thead class="bg-slate-900 text-slate-300">
              <tr>
                <th class="px-4 py-3 text-left font-semibold">Name</th>
                <th class="px-4 py-3 text-left font-semibold">Host</th>
                <th class="px-4 py-3 text-left font-semibold">Path</th>
                <th class="px-4 py-3 text-left font-semibold">LB Strategy</th>
                <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
                <th class="px-4 py-3 text-left font-semibold">Health</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-slate-800">
              {#if recentRoutes.length === 0}
                <tr>
                  <td class="px-4 py-8 text-center text-slate-400" colspan="6">
                    No routes configured yet.
                    <button
                      class="ml-2 text-cyan-400 hover:text-cyan-300"
                      on:click={() => dispatch('addRoute')}
                    >
                      Add your first route →
                    </button>
                  </td>
                </tr>
              {:else}
                {#each recentRoutes as route, idx}
                  <tr class="transition-colors hover:bg-slate-900/70">
                    <td class="px-4 py-3">
                      <span class="font-medium text-slate-100">{route.name}</span>
                      {#if route.is_default}
                        <span class="ml-2 rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-200">
                          default
                        </span>
                      {/if}
                    </td>
                    <td class="px-4 py-3 text-slate-300">
                      {#if route.host}
                        {route.host}
                      {:else}
                        <span class="text-slate-500">—</span>
                      {/if}
                    </td>
                    <td class="px-4 py-3 text-slate-300">
                      <code class="rounded bg-slate-800 px-1.5 py-0.5 text-xs">{route.path_prefix}</code>
                    </td>
                    <td class="px-4 py-3 text-slate-300">
                      {formatLbStrategy(route.lb)}
                    </td>
                    <td class="px-4 py-3 text-slate-300">
                      {route.upstreams?.length ?? 0}
                    </td>
                    <td class="px-4 py-3">
                      <span class={statusBadgeClass(routeHealthStatus(idx))}>
                        {routeHealthStatus(idx) === 'healthy' ? 'UP' : ''}
                        {routeHealthStatus(idx) === 'degraded' ? 'DEGRADED' : ''}
                        {routeHealthStatus(idx) === 'down' ? 'DOWN' : ''}
                        {routeHealthStatus(idx) === 'unknown' ? 'UNKNOWN' : ''}
                      </span>
                    </td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        </div>

        {#if hasMoreRoutes}
          <div class="border-t border-slate-700/80 px-4 py-3">
            <button
              class="w-full text-center text-sm font-medium text-slate-400 transition-colors hover:text-cyan-400"
              on:click={() => dispatch('navigate', 'routes')}
            >
              Showing {recentRoutes.length} of {routes.length} routes — View All →
            </button>
          </div>
        {/if}
      </div>
    </section>
  </div>
</AppLayout>