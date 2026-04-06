<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import StatsCard from '../dashboard/StatsCard.svelte';
  import type { PrxConfig, RouteConfig, ServiceConfig } from '../../types/config';
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

  $: services = config.services ?? [];

  $: routes = config.routes ?? [];

  $: totalServices = services.length;

  $: totalRoutes = routes.length;

  $: totalUpstreams = services.reduce(
    (sum, service) => sum + (service.upstreams?.length ?? 0),
    0
  );

  $: tlsEnabledCount = services.reduce(
    (sum, service) =>
      sum + (service.upstreams?.filter((u) => u.tls === true).length ?? 0),
    0
  );

  $: circuitBreakerCount = services.filter(
    (service) => service.circuit_breaker?.enabled === true
  ).length;

  // Service lookup by name
  $: serviceByName = (() => {
    const map: Record<string, ServiceConfig> = {};
    for (const service of services) {
      map[service.name] = service;
    }
    return map;
  })();

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

  // Unique services that have been health-checked
  $: healthCheckedServices = (() => {
    const serviceSet = new Set<string>();
    for (const item of healthRoutes) {
      serviceSet.add(item.service);
    }
    return serviceSet.size;
  })();

  // Recent routes (first 5)
  $: recentRoutes = routes.slice(0, 5);

  $: hasMoreRoutes = routes.length > 5;

  // Recent services (first 3)
  $: recentServices = services.slice(0, 3);

  $: hasMoreServices = services.length > 3;

  // Health data by route index for quick lookup
  $: healthByIndex = (() => {
    const map: Record<number, RouteHealthItem> = {};
    for (const item of healthRoutes) {
      map[item.route_index] = item;
    }
    return map;
  })();

  // Health data by service name for service preview
  $: healthByService = (() => {
    const map: Record<string, RouteHealthItem> = {};
    for (const item of healthRoutes) {
      if (!map[item.service]) {
        map[item.service] = item;
      }
    }
    return map;
  })();

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  const getRouteHealth = (routeIndex: number): RouteHealthItem | null =>
    healthByIndex[routeIndex] ?? null;

  const getServiceHealth = (serviceName: string): RouteHealthItem | null =>
    healthByService[serviceName] ?? null;

  const routeHealthStatus = (routeIndex: number): 'healthy' | 'degraded' | 'down' | 'unknown' => {
    const health = getRouteHealth(routeIndex);
    if (!health) return 'unknown';
    if (health.reachable_upstreams === 0) return 'down';
    if (health.reachable_upstreams < health.total_upstreams) return 'degraded';
    return health.healthy ? 'healthy' : 'degraded';
  };

  const serviceHealthStatus = (serviceName: string): 'healthy' | 'degraded' | 'down' | 'unknown' => {
    const health = getServiceHealth(serviceName);
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

  const formatMethods = (methods: string[]): string => {
    if (!methods || methods.length === 0) return 'All';
    return methods.map((m) => m.toUpperCase()).join(', ');
  };

  const healthStatusText = (status: string): string => {
    switch (status) {
      case 'healthy':
        return 'UP';
      case 'degraded':
        return 'DEGRADED';
      case 'down':
        return 'DOWN';
      default:
        return 'UNKNOWN';
    }
  };

  const healthCheckedAt = (): string => {
    if (!routeHealth?.checked_at_epoch_ms) return '';
    const date = new Date(routeHealth.checked_at_epoch_ms);
    return date.toLocaleTimeString();
  };

  const getServiceForRoute = (route: RouteConfig): ServiceConfig | null =>
    serviceByName[route.service] ?? null;

  const healthTooltipDetail = (status: string): string => {
    switch (status) {
      case 'healthy':
        return 'All upstreams reachable';
      case 'degraded':
        return 'Partial upstream failure';
      case 'down':
        return 'No upstreams reachable';
      default:
        return 'Not yet checked';
    }
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
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
        <StatsCard
          icon="◆"
          label="Services"
          value={totalServices}
          color="cyan"
        />
        <StatsCard
          icon="⇌"
          label="Routes"
          value={totalRoutes}
          color="violet"
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
          color="amber"
        />
        <StatsCard
          icon="⚡"
          label="Circuit Breakers"
          value={circuitBreakerCount}
          color="rose"
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
          <div class="text-xs text-slate-500">
            {healthCheckedServices} of {totalServices} services checked
          </div>
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

        <!-- Route Health Dots (grouped by service health) -->
        <div class="flex flex-wrap items-center gap-2">
          {#each routes as route, idx}
            <div
              class="group relative"
              title="{route.name}: {routeHealthStatus(idx)}"
            >
              <span class="h-3 w-3 rounded-full {statusDotClass(routeHealthStatus(idx))} transition-transform hover:scale-125" />
              <!-- Tooltip on hover -->
              <div class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-2 -translate-x-1/2 whitespace-nowrap rounded-lg border border-slate-600 bg-slate-800 px-2 py-1 text-xs text-slate-200 opacity-0 shadow-lg transition-opacity group-hover:opacity-100">
                <div class="font-medium">{route.name}</div>
                <div class="mt-0.5 text-slate-400">
                  Service: {route.service}
                </div>
                <div class="mt-0.5 text-slate-400">
                  {healthTooltipDetail(routeHealthStatus(idx))}
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
          on:click={() => dispatch('navigate', 'services')}
        >
          <span class="text-base">◆</span>
          New Service
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-xl border border-violet-400/40 bg-violet-500/10 px-4 py-2.5 text-sm font-semibold text-violet-200 transition-colors hover:bg-violet-500/20"
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

    <!-- Recent Services Preview -->
    <section>
      <div class="mb-3 flex items-center justify-between">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-slate-400">
          Recent Services
        </h2>
        {#if hasMoreServices}
          <button
            class="text-sm font-medium text-cyan-400 transition-colors hover:text-cyan-300"
            on:click={() => dispatch('navigate', 'services')}
          >
            View All Services →
          </button>
        {/if}
      </div>

      <div class="rounded-2xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
        <div class="grid grid-cols-1 divide-y divide-slate-800 sm:grid-cols-3 sm:divide-x sm:divide-y-0">
          {#if recentServices.length === 0}
            <div class="px-4 py-8 text-center text-slate-400 sm:col-span-3">
              No services configured yet.
              <button
                class="ml-2 text-cyan-400 hover:text-cyan-300"
                on:click={() => dispatch('navigate', 'services')}
              >
                Add your first service →
              </button>
            </div>
          {:else}
            {#each recentServices as service}
              <div class="flex flex-col gap-2 px-4 py-4">
                <div class="flex items-center justify-between">
                  <span class="font-medium text-slate-100">{service.name}</span>
                  <span class="h-2 w-2 rounded-full {statusDotClass(serviceHealthStatus(service.name))}" />
                </div>
                <div class="flex items-center gap-3 text-xs text-slate-400">
                  <span>{formatLbStrategy(service.lb)}</span>
                  <span>·</span>
                  <span>{service.upstreams.length} upstream{service.upstreams.length !== 1 ? 's' : ''}</span>
                </div>
                {#if serviceHealthStatus(service.name) !== 'unknown'}
                  <span class={statusBadgeClass(serviceHealthStatus(service.name))}>
                    {healthStatusText(serviceHealthStatus(service.name))}
                  </span>
                {/if}
              </div>
            {/each}
          {/if}
        </div>

        {#if hasMoreServices}
          <div class="border-t border-slate-700/80 px-4 py-3">
            <button
              class="w-full text-center text-sm font-medium text-slate-400 transition-colors hover:text-cyan-400"
              on:click={() => dispatch('navigate', 'services')}
            >
              Showing {recentServices.length} of {services.length} services — View All →
            </button>
          </div>
        {/if}
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
                <th class="px-4 py-3 text-left font-semibold">Service</th>
                <th class="px-4 py-3 text-left font-semibold">Host</th>
                <th class="px-4 py-3 text-left font-semibold">Path</th>
                <th class="px-4 py-3 text-left font-semibold">Methods</th>
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
                    <td class="px-4 py-3">
                      <span class="rounded bg-cyan-500/10 px-1.5 py-0.5 text-xs font-medium text-cyan-300">
                        {route.service}
                      </span>
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
                    <td class="px-4 py-3">
                      {#if route.methods.length === 0}
                        <span class="text-xs text-slate-500">All</span>
                      {:else}
                        <div class="flex flex-wrap gap-1">
                          {#each route.methods as method}
                            <span class="rounded bg-slate-800 px-1.5 py-0.5 text-[10px] font-semibold text-slate-400">
                              {method.toUpperCase()}
                            </span>
                          {/each}
                        </div>
                      {/if}
                    </td>
                    <td class="px-4 py-3">
                      <span class={statusBadgeClass(routeHealthStatus(idx))}>
                        {healthStatusText(routeHealthStatus(idx))}
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