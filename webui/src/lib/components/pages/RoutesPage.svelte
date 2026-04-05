<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import RouteDetailPanel from '../routes/RouteDetailPanel.svelte';
  import type { PrxConfig, RouteConfig } from '../../types/config';
  import type { RouteHealthItem } from '../../api/admin';
  import type { NavPage } from '../../stores/navigation';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export let config: PrxConfig;
  export let routeHealthByIndex: Record<number, RouteHealthItem> = {};
  export let healthLoading: boolean = false;
  export let healthError: string = '';

  // ---------------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------------

  const dispatch = createEventDispatcher<{
    addRoute: void;
    viewRoute: number;
    editRoute: number;
    deleteRoute: number;
    duplicateRoute: number;
    refreshHealth: void;
    navigate: NavPage;
  }>();

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let searchQuery = '';
  let selectedRouteIndex: number | null = null;
  let detailMode: 'view' | 'edit' = 'edit';
  let currentPage = 1;
  const pageSize = 10;

  // ---------------------------------------------------------------------------
  // Computed Values
  // ---------------------------------------------------------------------------

  $: routes = config.routes ?? [];

  interface FilteredRoute {
    route: RouteConfig;
    index: number;
  }

  $: filteredRoutes = routes
    .map((route, index) => ({ route, index }))
    .filter(({ route }) => {
      if (!searchQuery.trim()) return true;
      const query = searchQuery.toLowerCase();
      return (
        route.name.toLowerCase().includes(query) ||
        route.host.toLowerCase().includes(query) ||
        route.path_prefix.toLowerCase().includes(query) ||
        route.lb.toLowerCase().includes(query)
      );
    });

  $: totalPages = Math.max(1, Math.ceil(filteredRoutes.length / pageSize));

  $: paginatedRoutes = (() => {
    const start = (currentPage - 1) * pageSize;
    return filteredRoutes.slice(start, start + pageSize);
  })();

  // Reset to page 1 when search changes
  $: if (searchQuery) {
    currentPage = 1;
  }

  // Ensure current page is valid
  $: if (currentPage > totalPages) {
    currentPage = totalPages;
  }

  // ---------------------------------------------------------------------------
  // View State
  // ---------------------------------------------------------------------------

  $: isDetailView = selectedRouteIndex !== null;
  $: selectedRoute = selectedRouteIndex !== null ? routes[selectedRouteIndex] ?? null : null;

  // ---------------------------------------------------------------------------
  // Health Helpers
  // ---------------------------------------------------------------------------

  const getRouteHealth = (index: number): RouteHealthItem | null =>
    routeHealthByIndex[index] ?? null;

  const routeHealthStatus = (index: number): 'up' | 'degraded' | 'down' | 'unknown' => {
    const health = getRouteHealth(index);
    if (!health) return 'unknown';
    if (health.reachable_upstreams === 0) return 'down';
    if (health.reachable_upstreams < health.total_upstreams) return 'degraded';
    return health.healthy ? 'up' : 'degraded';
  };

  const healthStatusLabel = (status: string): string => {
    switch (status) {
      case 'up': return 'UP';
      case 'degraded': return 'DEGRADED';
      case 'down': return 'DOWN';
      default: return 'UNKNOWN';
    }
  };

  const healthBadgeClass = (status: string): string => {
    switch (status) {
      case 'up':
        return 'rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-200';
      case 'degraded':
        return 'rounded-full border border-amber-400/40 bg-amber-500/10 px-2 py-0.5 text-xs font-semibold text-amber-200';
      case 'down':
        return 'rounded-full border border-rose-400/40 bg-rose-500/10 px-2 py-0.5 text-xs font-semibold text-rose-200';
      default:
        return 'rounded-full border border-slate-500 bg-slate-800 px-2 py-0.5 text-xs font-semibold text-slate-300';
    }
  };

  const healthDotClass = (healthy: boolean): string =>
    healthy
      ? 'h-2 w-2 rounded-full bg-emerald-400'
      : 'h-2 w-2 rounded-full bg-rose-400';

  const healthTooltip = (index: number): string => {
    const health = getRouteHealth(index);
    if (!health) return 'Not yet checked';
    return health.upstreams
      .map((upstream) =>
        upstream.healthy
          ? `${upstream.addr}: UP (${upstream.latency_ms ?? 0}ms)`
          : `${upstream.addr}: DOWN (${upstream.error ?? 'unreachable'})`
      )
      .join('\n');
  };

  // ---------------------------------------------------------------------------
  // Formatting Helpers
  // ---------------------------------------------------------------------------

  const formatLbStrategy = (lb: string): string => {
    switch (lb) {
      case 'round_robin': return 'Round Robin';
      case 'random': return 'Random';
      case 'hash': return 'Hash';
      default: return lb;
    }
  };

  const cbBadgeClass = (enabled: boolean): string =>
    enabled
      ? 'rounded-full border border-amber-400/40 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-amber-200'
      : 'rounded-full border border-slate-600 bg-slate-800 px-1.5 py-0.5 text-[10px] font-semibold text-slate-500';

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  const handleSearch = (event: Event) => {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  };

  const handleRowClick = (index: number) => {
    selectedRouteIndex = index;
  };

  const handleViewRoute = (index: number) => {
    detailMode = 'view';
    selectedRouteIndex = index;
  };

  const handleEditRoute = (index: number) => {
    detailMode = 'edit';
    selectedRouteIndex = index;
  };

  const handleDeleteRoute = (index: number) => {
    dispatch('deleteRoute', index);
    if (selectedRouteIndex === index) {
      selectedRouteIndex = null;
    }
  };

  const handleDuplicateRoute = (index: number) => {
    dispatch('duplicateRoute', index);
  };

  const handleCloseDetail = () => {
    selectedRouteIndex = null;
  };

  const handleDetailDelete = (e: CustomEvent<number>) => {
    dispatch('deleteRoute', e.detail);
    selectedRouteIndex = null;
  };

  const goToPage = (page: number) => {
    if (page >= 1 && page <= totalPages) {
      currentPage = page;
    }
  };

  const getPageNumbers = (): (number | '...')[] => {
    const pages: (number | '...')[] = [];
    if (totalPages <= 7) {
      for (let i = 1; i <= totalPages; i++) pages.push(i);
    } else {
      pages.push(1);
      if (currentPage > 3) pages.push('...');
      const start = Math.max(2, currentPage - 1);
      const end = Math.min(totalPages - 1, currentPage + 1);
      for (let i = start; i <= end; i++) pages.push(i);
      if (currentPage < totalPages - 2) pages.push('...');
      pages.push(totalPages);
    }
    return pages;
  };
</script>

<AppLayout title="Routes" subtitle="Manage your proxy routes and upstreams">
  <svelte:fragment slot="header-actions">
    <button
      class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
      on:click={() => dispatch('addRoute')}
    >
      + Add Route
    </button>
    <button
      class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={healthLoading}
      on:click={() => dispatch('refreshHealth')}
    >
      {healthLoading ? 'Checking...' : '↻ Check Health'}
    </button>
  </svelte:fragment>

  <div class="p-6">
    {#if isDetailView && selectedRoute}
      <!-- Detail View -->
      <div class="mx-auto max-w-4xl">
        <RouteDetailPanel
          route={selectedRoute}
          routeIndex={selectedRouteIndex ?? 0}
          mode={detailMode}
          on:close={handleCloseDetail}
          on:deleteRoute={handleDetailDelete}
        />
      </div>
    {:else}
      <!-- List View -->
      <div class="space-y-4">
        <!-- Search Bar -->
        <div class="flex items-center gap-4">
          <div class="relative flex-1">
            <span class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-500">⌕</span>
            <input
              type="text"
              class="w-full rounded-lg border border-slate-600 bg-slate-900 py-2.5 pl-9 pr-4 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30"
              placeholder="Search by name, host, path, or LB strategy..."
              value={searchQuery}
              on:input={handleSearch}
            />
          </div>
          <div class="flex items-center gap-2 text-sm text-slate-400">
            <span class="rounded-lg border border-slate-700 bg-slate-800/60 px-3 py-2 tabular-nums">
              {filteredRoutes.length} route{filteredRoutes.length !== 1 ? 's' : ''}
            </span>
          </div>
        </div>

        <!-- Health Error Banner -->
        {#if healthError}
          <div class="rounded-lg border border-rose-400/40 bg-rose-500/10 px-4 py-3 text-sm font-medium text-rose-200">
            <span class="mr-2">⚠</span>
            Health check failed: {healthError}
          </div>
        {/if}

        <!-- Routes Table -->
        <div class="overflow-hidden rounded-xl border border-slate-700 bg-slate-950/70">
          {#if filteredRoutes.length === 0}
            <!-- Empty State -->
            <div class="flex flex-col items-center justify-center px-6 py-20">
              <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl border border-slate-700 bg-slate-800/60">
                <span class="text-3xl text-slate-500">⇌</span>
              </div>
              {#if searchQuery}
                <h3 class="text-base font-semibold text-slate-300">No routes match your search</h3>
                <p class="mt-1 text-sm text-slate-500">
                  Try adjusting your search terms or
                  <button
                    class="ml-1 text-cyan-400 transition-colors hover:text-cyan-300"
                    on:click={() => searchQuery = ''}
                  >
                    clear the filter
                  </button>
                </p>
              {:else}
                <h3 class="text-base font-semibold text-slate-300">No routes configured</h3>
                <p class="mt-1 text-sm text-slate-500">
                  Get started by adding your first route.
                </p>
                <button
                  class="mt-4 inline-flex items-center gap-2 rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
                  on:click={() => dispatch('addRoute')}
                >
                  <span>+</span>
                  Add Route
                </button>
              {/if}
            </div>
          {:else}
            <div class="overflow-x-auto">
              <table class="min-w-full divide-y divide-slate-800 text-sm">
                <thead class="bg-slate-900 text-slate-300">
                  <tr>
                    <th class="px-4 py-3 text-left font-semibold">Name</th>
                    <th class="px-4 py-3 text-left font-semibold">Host</th>
                    <th class="px-4 py-3 text-left font-semibold">Path</th>
                    <th class="px-4 py-3 text-left font-semibold">LB Strategy</th>
                    <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
                    <th class="px-4 py-3 text-left font-semibold">Circuit Breaker</th>
                    <th class="px-4 py-3 text-left font-semibold">Health</th>
                    <th class="px-4 py-3 text-right font-semibold">Actions</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-slate-800">
                  {#each paginatedRoutes as { route, index } (index)}
                    {@const healthStatus = routeHealthStatus(index)}
                    {@const health = getRouteHealth(index)}
                    <tr
                      class="cursor-pointer transition-colors {selectedRouteIndex === index ? 'bg-cyan-500/10' : 'hover:bg-slate-900/70'}"
                      on:click={() => handleRowClick(index)}
                      on:keydown={(e) => e.key === 'Enter' && handleRowClick(index)}
                      role="button"
                      tabindex="0"
                    >
                      <!-- Name -->
                      <td class="px-4 py-3">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-slate-100">{route.name}</span>
                          {#if route.is_default}
                            <span class="rounded-full border border-emerald-400/40 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-200">
                              default
                            </span>
                          {/if}
                        </div>
                      </td>

                      <!-- Host -->
                      <td class="px-4 py-3 text-slate-300">
                        {#if route.host}
                          {route.host}
                        {:else}
                          <span class="text-slate-600">—</span>
                        {/if}
                      </td>

                      <!-- Path -->
                      <td class="px-4 py-3">
                        <code class="rounded bg-slate-800/80 px-1.5 py-0.5 text-xs font-mono text-slate-300">
                          {route.path_prefix}
                        </code>
                      </td>

                      <!-- LB Strategy -->
                      <td class="px-4 py-3 text-slate-300">
                        {formatLbStrategy(route.lb)}
                      </td>

                      <!-- Upstreams -->
                      <td class="px-4 py-3">
                        <div class="flex items-center gap-2">
                          <span class="text-slate-400">{route.upstreams.length}</span>
                          <div class="flex items-center gap-1" title={healthTooltip(index)}>
                            {#if health}
                              {#each health.upstreams as upstream}
                                <span class={healthDotClass(upstream.healthy)} />
                              {/each}
                            {:else}
                              <span class="text-xs text-slate-600">n/a</span>
                            {/if}
                          </div>
                        </div>
                      </td>

                      <!-- Circuit Breaker -->
                      <td class="px-4 py-3">
                        <span class={cbBadgeClass(route.circuit_breaker.enabled)}>
                          {route.circuit_breaker.enabled ? 'ON' : 'OFF'}
                        </span>
                      </td>

                      <!-- Health Status -->
                      <td class="px-4 py-3">
                        <span class={healthBadgeClass(healthStatus)} title={healthTooltip(index)}>
                          {healthStatusLabel(healthStatus)}
                        </span>
                      </td>

                      <!-- Actions -->
                      <td class="px-4 py-3">
                        <div class="flex items-center justify-end gap-1.5">
                          <button
                            class="rounded-md border border-slate-600 bg-slate-900 px-2 py-1 text-xs font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100"
                            on:click|stopPropagation={() => handleViewRoute(index)}
                            title="View route"
                          >
                            View
                          </button>
                          <button
                            class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-2 py-1 text-xs font-medium text-cyan-200 transition-colors hover:bg-cyan-500/20"
                            on:click|stopPropagation={() => handleEditRoute(index)}
                            title="Edit route"
                          >
                            Edit
                          </button>
                          <button
                            class="rounded-md border border-slate-600 bg-slate-900 px-2 py-1 text-xs font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100"
                            on:click|stopPropagation={() => handleDuplicateRoute(index)}
                            title="Duplicate route"
                          >
                            Dup
                          </button>
                          <button
                            class="rounded-md border border-rose-400/40 bg-rose-500/10 px-2 py-1 text-xs font-medium text-rose-200 transition-colors hover:bg-rose-500/20"
                            on:click|stopPropagation={() => handleDeleteRoute(index)}
                            title="Delete route"
                          >
                            Del
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <!-- Pagination -->
            {#if totalPages > 1}
              <div class="flex items-center justify-between border-t border-slate-800 bg-slate-900/50 px-4 py-3">
                <p class="text-xs text-slate-500">
                  Showing {(currentPage - 1) * pageSize + 1}–{Math.min(currentPage * pageSize, filteredRoutes.length)} of {filteredRoutes.length}
                </p>
                <div class="flex items-center gap-1">
                  <button
                    class="rounded-md border border-slate-600 bg-slate-800 px-2 py-1 text-xs font-medium text-slate-400 transition-colors hover:bg-slate-700 hover:text-slate-200 disabled:cursor-not-allowed disabled:opacity-40"
                    disabled={currentPage === 1}
                    on:click={() => goToPage(currentPage - 1)}
                  >
                    ←
                  </button>
                  {#each getPageNumbers() as page}
                    {#if page === '...'}
                      <span class="px-1 text-xs text-slate-500">…</span>
                    {:else}
                      <button
                        class="rounded-md border px-2 py-1 text-xs font-medium transition-colors {page === currentPage ? 'border-cyan-500/50 bg-cyan-500/10 text-cyan-200' : 'border-slate-600 bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-slate-200'}"
                        on:click={() => goToPage(page)}
                      >
                        {page}
                      </button>
                    {/if}
                  {/each}
                  <button
                    class="rounded-md border border-slate-600 bg-slate-800 px-2 py-1 text-xs font-medium text-slate-400 transition-colors hover:bg-slate-700 hover:text-slate-200 disabled:cursor-not-allowed disabled:opacity-40"
                    disabled={currentPage === totalPages}
                    on:click={() => goToPage(currentPage + 1)}
                  >
                    →
                  </button>
                </div>
              </div>
            {/if}
          {/if}
        </div>

        <!-- Table Footer Info -->
        {#if routes.length > 0 && filteredRoutes.length === 0 && searchQuery}
          <div class="text-center text-sm text-slate-500">
            No results found for "<span class="text-slate-400">{searchQuery}</span>"
          </div>
        {/if}
      </div>
    {/if}
  </div>
</AppLayout>