<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import RouteDetailPanel from '../routes/RouteDetailPanel.svelte';
  import RouteFormModal from '../routes/RouteFormModal.svelte';
  import { createRoute, updateRoute, deleteRoute, loadConfigFromAdmin } from '../../api/admin';
  import { configStore } from '../../stores/config';
  import type { PrxConfig, RouteConfig, ServiceConfig } from '../../types/config';
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
  let isSaving = false;
  let errorMessage = '';
  let showCreateModal = false;
  let isRefreshing = false;

  // ---------------------------------------------------------------------------
  // Computed Values
  // ---------------------------------------------------------------------------

  $: routes = config.routes ?? [];
  $: services = config.services ?? [];

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
        route.service.toLowerCase().includes(query) ||
        route.methods.some((m) => m.toLowerCase().includes(query))
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
  $: selectedRoute = selectedRouteIndex !== null
    ? routes[selectedRouteIndex] ?? null
    : null;

  // ---------------------------------------------------------------------------
  // API Operations
  // ---------------------------------------------------------------------------

  async function refreshConfig(): Promise<void> {
    isRefreshing = true;
    errorMessage = '';
    try {
      const newConfig = await loadConfigFromAdmin();
      configStore.set(newConfig);
    } catch (err) {
      errorMessage = (err as Error).message || 'Failed to refresh configuration';
    } finally {
      isRefreshing = false;
    }
  }

  function clearError(): void {
    errorMessage = '';
  }

  // ---------------------------------------------------------------------------
  // Health Helpers
  // ---------------------------------------------------------------------------

  const getRouteHealth = (index: number): RouteHealthItem | null =>
    routeHealthByIndex[index] ?? null;

  const routeHealthStatus = (
    index: number
  ): 'up' | 'degraded' | 'down' | 'unknown' => {
    const health = getRouteHealth(index);
    if (!health) return 'unknown';
    if (health.reachable_upstreams === 0) return 'down';
    if (health.reachable_upstreams < health.total_upstreams) return 'degraded';
    return health.healthy ? 'up' : 'degraded';
  };

  const healthDotClass = (status: string): string => {
    switch (status) {
      case 'up':
        return 'h-2 w-2 rounded-full bg-emerald-400';
      case 'degraded':
        return 'h-2 w-2 rounded-full bg-amber-400';
      case 'down':
        return 'h-2 w-2 rounded-full bg-rose-400';
      default:
        return 'h-2 w-2 rounded-full bg-slate-600';
    }
  };

  const healthTooltip = (index: number): string => {
    const health = getRouteHealth(index);
    if (!health) return 'Health not checked';
    const reachable = health.reachable_upstreams;
    const total = health.total_upstreams;
    if (total === 0) return 'No upstreams';
    return `${reachable}/${total} upstreams reachable`;
  };

  // ---------------------------------------------------------------------------
  // Formatting Helpers
  // ---------------------------------------------------------------------------

  const methodBadgeColor = (method: string): string => {
    const upper = method.toUpperCase();
    if (upper === 'GET') return 'border-emerald-500/40 bg-emerald-500/10 text-emerald-200';
    if (upper === 'POST') return 'border-cyan-500/40 bg-cyan-500/10 text-cyan-200';
    if (upper === 'PUT') return 'border-amber-500/40 bg-amber-500/10 text-amber-200';
    if (upper === 'DELETE') return 'border-rose-500/40 bg-rose-500/10 text-rose-200';
    if (upper === 'PATCH') return 'border-violet-500/40 bg-violet-500/10 text-violet-200';
    return 'border-slate-500/40 bg-slate-500/10 text-slate-300';
  };

  const serviceExists = (serviceName: string): boolean =>
    services.some((s) => s.name === serviceName);

  // ---------------------------------------------------------------------------
  // Class Helpers (avoid class: with /)
  // ---------------------------------------------------------------------------

  const rowClass = (index: number): string =>
    selectedRouteIndex === index
      ? 'cursor-pointer transition-colors bg-cyan-500/10'
      : 'cursor-pointer transition-colors hover:bg-slate-900/70';

  const pageBtnClass = (page: number): string =>
    page === currentPage
      ? 'rounded-md border px-2 py-1 text-xs font-medium transition-colors border-cyan-500/50 bg-cyan-500/10 text-cyan-200'
      : 'rounded-md border px-2 py-1 text-xs font-medium transition-colors border-slate-600 bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-slate-200';

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  const handleSearch = (event: Event) => {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  };

  const handleRowClick = (index: number) => {
    detailMode = 'edit';
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

  const handleDeleteRoute = async (index: number) => {
    if (isSaving) return;
    const route = routes[index];
    if (!route) return;

    isSaving = true;
    errorMessage = '';
    try {
      await deleteRoute(route.name);
      selectedRouteIndex = null;
      await refreshConfig();
    } catch (err) {
      errorMessage = (err as Error).message || 'Failed to delete route';
    } finally {
      isSaving = false;
    }
  };

  const handleDuplicateRoute = async (index: number) => {
    if (isSaving) return;
    const originalRoute = routes[index];
    if (!originalRoute) return;

    const newRoute: RouteConfig = {
      ...originalRoute,
      name: `${originalRoute.name}-copy`,
      is_default: false,
    };

    isSaving = true;
    errorMessage = '';
    try {
      await createRoute(newRoute);
      await refreshConfig();
    } catch (err) {
      errorMessage = (err as Error).message || 'Failed to duplicate route';
    } finally {
      isSaving = false;
    }
  };

  const handleCloseDetail = () => {
    selectedRouteIndex = null;
    clearError();
  };

  const handleDetailDelete = async (e: CustomEvent<number>) => {
    await handleDeleteRoute(e.detail);
  };

  const handleDetailSave = async (e: CustomEvent<RouteConfig>) => {
    if (isSaving) return;
    const route = e.detail;
    const originalName = routes[selectedRouteIndex ?? 0]?.name;
    if (!originalName) return;

    isSaving = true;
    errorMessage = '';
    try {
      await updateRoute(originalName, route);
      selectedRouteIndex = null;
      await refreshConfig();
    } catch (err) {
      errorMessage = (err as Error).message || 'Failed to update route';
    } finally {
      isSaving = false;
    }
  };

  const handleNavigateServices = () => {
    dispatch('navigate', 'services');
  };

  // ---------------------------------------------------------------------------
  // Create Modal Handlers
  // ---------------------------------------------------------------------------

  const handleOpenCreateModal = () => {
    clearError();
    showCreateModal = true;
  };

  const handleModalSave = async (e: CustomEvent<RouteConfig>) => {
    showCreateModal = false;
    const route = e.detail;
    isSaving = true;
    errorMessage = '';
    try {
      await createRoute(route);
      await refreshConfig();
    } catch (err) {
      errorMessage = (err as Error).message || 'Failed to create route';
    } finally {
      isSaving = false;
    }
  };

  const handleModalCancel = () => {
    showCreateModal = false;
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

<AppLayout title="Routes" subtitle="Manage routing rules that match requests to services">
  <svelte:fragment slot="header-actions">
    <button
      class="rounded-md border border-slate-600 bg-slate-800 px-3 py-1.5 text-xs font-semibold text-slate-300 transition-colors hover:bg-slate-700 hover:text-slate-100 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={isRefreshing || isSaving}
      on:click={refreshConfig}
      title="Refresh from server"
    >
      {isRefreshing ? '⟳' : '↻'} Refresh
    </button>
    <button
      class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={isSaving}
      on:click={handleOpenCreateModal}
    >
      + Add Route
    </button>
    <button
      class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={healthLoading || isSaving}
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
          services={services}
          saving={isSaving}
          errorMessage={errorMessage}
          on:close={handleCloseDetail}
          on:deleteRoute={handleDetailDelete}
          on:save={handleDetailSave}
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
              placeholder="Search by name, host, path, service, or method..."
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

        {#if errorMessage}
          <div class="rounded-lg border border-rose-400/40 bg-rose-500/10 px-4 py-3 text-sm font-medium text-rose-200">
            <span class="mr-2">⚠</span>
            {errorMessage}
            <button
              class="ml-2 rounded border border-rose-400/30 bg-rose-500/10 px-2 py-0.5 text-xs text-rose-300 transition-colors hover:bg-rose-500/20"
              on:click={clearError}
            >
              Dismiss
            </button>
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
                    on:click={() => (searchQuery = '')}
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
                  class="mt-4 inline-flex items-center gap-2 rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:cursor-not-allowed disabled:opacity-60"
                  disabled={isSaving}
                  on:click={handleOpenCreateModal}
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
                    <th class="px-4 py-3 text-left font-semibold">Service</th>
                    <th class="px-4 py-3 text-left font-semibold">Host</th>
                    <th class="px-4 py-3 text-left font-semibold">Path</th>
                    <th class="px-4 py-3 text-left font-semibold">Methods</th>
                    <th class="px-4 py-3 text-right font-semibold">Actions</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-slate-800">
                  {#each paginatedRoutes as { route, index } (index)}
                    <tr
                      class={rowClass(index)}
                      on:click={() => handleRowClick(index)}
                      on:keydown={(e) => e.key === 'Enter' && handleRowClick(index)}
                      role="button"
                      tabindex="0"
                    >
                      <!-- Name with health dot and default badge -->
                      <td class="px-4 py-3">
                        <div class="flex items-center gap-2">
                          <span
                            class={healthDotClass(routeHealthStatus(index))}
                            title={healthTooltip(index)}
                          />
                          <span class="font-medium text-slate-100">{route.name}</span>
                          {#if route.is_default}
                            <span class="rounded-full border border-emerald-400/40 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-200">
                              default
                            </span>
                          {/if}
                        </div>
                      </td>

                      <!-- Service -->
                      <td class="px-4 py-3">
                        {#if serviceExists(route.service)}
                          <button
                            class="rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-2 py-0.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
                            on:click|stopPropagation={handleNavigateServices}
                            title="View in Services"
                          >
                            {route.service}
                          </button>
                        {:else}
                          <span
                            class="rounded-lg border border-rose-400/40 bg-rose-500/10 px-2 py-0.5 text-xs font-semibold text-rose-200"
                            title="Service not found"
                          >
                            {route.service || '—'}
                          </span>
                        {/if}
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

                      <!-- Methods -->
                      <td class="px-4 py-3">
                        {#if route.methods.length === 0}
                          <span class="text-xs text-slate-500">All</span>
                        {:else}
                          <div class="flex flex-wrap gap-1">
                            {#each route.methods as method}
                              <span class="rounded border px-1.5 py-0.5 text-[10px] font-semibold {methodBadgeColor(method)}">
                                {method}
                              </span>
                            {/each}
                          </div>
                        {/if}
                      </td>

                      <!-- Actions -->
                      <td class="px-4 py-3">
                        <div class="flex items-center justify-end gap-1.5">
                          <button
                            class="rounded-md border border-slate-600 bg-slate-900 px-2 py-1 text-xs font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={isSaving}
                            on:click|stopPropagation={() => handleViewRoute(index)}
                            title="View route"
                          >
                            View
                          </button>
                          <button
                            class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-2 py-1 text-xs font-medium text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={isSaving}
                            on:click|stopPropagation={() => handleEditRoute(index)}
                            title="Edit route"
                          >
                            Edit
                          </button>
                          <button
                            class="rounded-md border border-slate-600 bg-slate-900 px-2 py-1 text-xs font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={isSaving}
                            on:click|stopPropagation={() => handleDuplicateRoute(index)}
                            title="Duplicate route"
                          >
                            Dup
                          </button>
                          <button
                            class="rounded-md border border-rose-400/40 bg-rose-500/10 px-2 py-1 text-xs font-medium text-rose-200 transition-colors hover:bg-rose-500/20 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={isSaving}
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
                        class={pageBtnClass(page)}
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

{#if showCreateModal}
  <RouteFormModal
    route={null}
    existingNames={routes.map((r) => r.name)}
    services={services}
    on:save={handleModalSave}
    on:cancel={handleModalCancel}
  />
{/if}