<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import ServiceFormModal from '../services/ServiceFormModal.svelte';
  import {
    configStore,
    addServiceUpstream,
    removeServiceUpstream
  } from '../../stores/config';
  import {
    createService,
    updateService,
    deleteService,
    loadConfigFromAdmin
  } from '../../api/admin';
  import type { LbStrategy, PrxConfig, ServiceConfig } from '../../types/config';
  import type { NavPage } from '../../stores/navigation';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export let config: PrxConfig;

  // ---------------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------------

  const dispatch = createEventDispatcher<{
    navigate: NavPage;
  }>();

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let searchQuery = '';
  let selectedServiceIndex: number | null = null;
  let circuitBreakerExpanded = false;
  let advancedUpstreamExpanded: Record<number, boolean> = {};
  let showDeleteConfirm = false;

  // API/Loading state
  let isSaving = false;
  let isLoading = false;
  let errorMessage = '';
  let errorTimeout: ReturnType<typeof setTimeout> | null = null;

  // Modal state
  let showCreateModal = false;

  const lbOptions: LbStrategy[] = ['round_robin', 'random', 'hash'];

  // ---------------------------------------------------------------------------
  // Computed Values
  // ---------------------------------------------------------------------------

  $: services = config.services ?? [];

  interface FilteredService {
    service: ServiceConfig;
    index: number;
  }

  $: filteredServices = services
    .map((service, index) => ({ service, index }))
    .filter(({ service }) => {
      if (!searchQuery.trim()) return true;
      const query = searchQuery.toLowerCase();
      return (
        service.name.toLowerCase().includes(query) ||
        service.lb.toLowerCase().includes(query)
      );
    });

  // ---------------------------------------------------------------------------
  // View State
  // ---------------------------------------------------------------------------

  $: isDetailView = selectedServiceIndex !== null;
  $: selectedService = selectedServiceIndex !== null
    ? services[selectedServiceIndex] ?? null
    : null;

  // Update circuit breaker expanded state when service changes
  $: if (selectedService) {
    circuitBreakerExpanded = selectedService.circuit_breaker.enabled;
  }

  // Count routes referencing a service
  const countRoutesForService = (serviceName: string): number => {
    return config.routes.filter((route) => route.service === serviceName).length;
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

  const rowClass = (index: number): string =>
    selectedServiceIndex === index
      ? 'cursor-pointer transition-colors bg-cyan-500/10'
      : 'cursor-pointer transition-colors hover:bg-slate-900/70';

  const collapseIconClass = (expanded: boolean): string =>
    expanded ? 'text-slate-400 transition-transform rotate-180' : 'text-slate-400 transition-transform';

  // ---------------------------------------------------------------------------
  // Input Helpers
  // ---------------------------------------------------------------------------

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;

  const numberValue = (event: Event, fallback = 0): number =>
    Number((event.currentTarget as HTMLInputElement).value || fallback);

  const checkedValue = (event: Event): boolean =>
    (event.currentTarget as HTMLInputElement).checked;

  const selectValue = (event: Event): string =>
    (event.currentTarget as HTMLSelectElement).value;

  const toNullableNumber = (value: string): number | null => {
    const trimmed = value.trim();
    if (!trimmed) return null;
    const num = Number(trimmed);
    if (!Number.isFinite(num)) return null;
    return Math.max(0, Math.floor(num));
  };

  // ---------------------------------------------------------------------------
  // Error Handling
  // ---------------------------------------------------------------------------

  const showError = (message: string) => {
    errorMessage = message;
    if (errorTimeout) {
      clearTimeout(errorTimeout);
    }
    errorTimeout = setTimeout(() => {
      errorMessage = '';
      errorTimeout = null;
    }, 5000);
  };

  const clearError = () => {
    errorMessage = '';
    if (errorTimeout) {
      clearTimeout(errorTimeout);
      errorTimeout = null;
    }
  };

  // ---------------------------------------------------------------------------
  // API Refresh
  // ---------------------------------------------------------------------------

  const refreshConfig = async () => {
    try {
      isLoading = true;
      clearError();
      const newConfig = await loadConfigFromAdmin();
      configStore.set(newConfig);
      config = newConfig;

      // Re-select the service if we were in detail view
      if (selectedService !== null) {
        const newIndex = newConfig.services.findIndex((s: ServiceConfig) => s.name === selectedService.name);
        selectedServiceIndex = newIndex >= 0 ? newIndex : null;
      }
    } catch (err) {
      showError(err instanceof Error ? err.message : 'Failed to refresh config');
    } finally {
      isLoading = false;
    }
  };

  // ---------------------------------------------------------------------------
  // Local Update Functions (for form editing in detail view)
  // ---------------------------------------------------------------------------

  const updateServiceLocal = <K extends keyof ServiceConfig>(key: K, value: ServiceConfig[K]) => {
    if (selectedServiceIndex === null) return;
    const idx = selectedServiceIndex;
    configStore.update((cfg) => {
      const target = cfg.services[idx];
      if (!target) return cfg;
      target[key] = value;
      return cfg;
    });
  };

  const updateServiceLb = (event: Event) => {
    updateServiceLocal('lb', selectValue(event) as LbStrategy);
  };

  const updateCircuitBreaker = (
    key: 'enabled' | 'consecutive_failures' | 'open_ms',
    value: number | boolean
  ) => {
    if (selectedServiceIndex === null) return;
    const idx = selectedServiceIndex;
    configStore.update((cfg) => {
      const service = cfg.services[idx];
      if (!service) return cfg;
      if (key === 'enabled') {
        service.circuit_breaker.enabled = Boolean(value);
        circuitBreakerExpanded = Boolean(value);
      } else if (key === 'consecutive_failures') {
        service.circuit_breaker.consecutive_failures = Math.max(1, Number(value));
      } else {
        service.circuit_breaker.open_ms = Math.max(1, Number(value));
      }
      return cfg;
    });
  };

  const updateUpstream = (
    upstreamIndex: number,
    key:
      | 'addr'
      | 'sni'
      | 'weight'
      | 'tls'
      | 'verify_cert'
      | 'verify_hostname'
      | 'connect_timeout_ms'
      | 'total_connect_timeout_ms'
      | 'read_timeout_ms'
      | 'write_timeout_ms'
      | 'idle_timeout_ms',
    value: string | number | boolean | null
  ) => {
    if (selectedServiceIndex === null) return;
    const idx = selectedServiceIndex;
    configStore.update((cfg) => {
      const upstream = cfg.services[idx]?.upstreams[upstreamIndex];
      if (!upstream) return cfg;
      if (key === 'weight' && typeof value === 'number') {
        upstream.weight = Math.min(256, Math.max(1, value));
      } else if (key === 'tls' && typeof value === 'boolean') {
        upstream.tls = value;
      } else if ((key === 'verify_cert' || key === 'verify_hostname') && typeof value === 'boolean') {
        upstream[key] = value;
      } else if (
        (key === 'connect_timeout_ms' ||
          key === 'total_connect_timeout_ms' ||
          key === 'read_timeout_ms' ||
          key === 'write_timeout_ms' ||
          key === 'idle_timeout_ms') &&
        (typeof value === 'number' || value === null)
      ) {
        upstream[key] = value;
      } else if ((key === 'addr' || key === 'sni') && typeof value === 'string') {
        upstream[key] = value;
      }
      return cfg;
    });
  };

  // ---------------------------------------------------------------------------
  // API Actions
  // ---------------------------------------------------------------------------

  const handleCreateService = async (serviceData: ServiceConfig) => {
    try {
      isSaving = true;
      clearError();
      await createService(serviceData);
      showCreateModal = false;
      await refreshConfig();
      // Select the newly created service
      const newIndex = config.services.findIndex(s => s.name === serviceData.name);
      if (newIndex >= 0) {
        selectedServiceIndex = newIndex;
      }
    } catch (err) {
      showError(err instanceof Error ? err.message : 'Failed to create service');
    } finally {
      isSaving = false;
    }
  };

  const handleSaveService = async () => {
    if (selectedServiceIndex === null || !selectedService) return;

    try {
      isSaving = true;
      clearError();
      const originalName = config.services[selectedServiceIndex]?.name;
      if (!originalName) return;

      await updateService(originalName, selectedService);
      await refreshConfig();
    } catch (err) {
      showError(err instanceof Error ? err.message : 'Failed to save service');
    } finally {
      isSaving = false;
    }
  };

  const handleDeleteService = async (serviceName: string) => {
    try {
      isSaving = true;
      clearError();
      await deleteService(serviceName);
      selectedServiceIndex = null;
      showDeleteConfirm = false;
      await refreshConfig();
    } catch (err) {
      showError(err instanceof Error ? err.message : 'Failed to delete service');
    } finally {
      isSaving = false;
    }
  };

  const handleDuplicateService = async (index: number) => {
    const source = services[index];
    if (!source) return;

    try {
      isSaving = true;
      clearError();
      const clone = JSON.parse(JSON.stringify(source)) as ServiceConfig;
      clone.name = `${clone.name}-copy`;
      await createService(clone);
      await refreshConfig();
    } catch (err) {
      showError(err instanceof Error ? err.message : 'Failed to duplicate service');
    } finally {
      isSaving = false;
    }
  };

  // ---------------------------------------------------------------------------
  // UI Actions
  // ---------------------------------------------------------------------------

  const handleSearch = (event: Event) => {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  };

  const handleRowClick = (index: number) => {
    selectedServiceIndex = index;
  };

  const handleViewService = (index: number) => {
    selectedServiceIndex = index;
  };

  const handleEditService = (index: number) => {
    selectedServiceIndex = index;
  };

  const handleDeleteFromList = (index: number) => {
    const serviceName = services[index]?.name ?? '';
    const routeCount = countRoutesForService(serviceName);
    const warning = routeCount > 0
      ? `This will also remove ${routeCount} route(s) referencing this service. `
      : '';
    if (!confirm(`${warning}Delete service "${serviceName}"?`)) {
      return;
    }
    handleDeleteService(serviceName);
    if (selectedServiceIndex === index) {
      selectedServiceIndex = null;
    } else if (selectedServiceIndex !== null && selectedServiceIndex > index) {
      selectedServiceIndex = selectedServiceIndex - 1;
    }
  };

  const handleOpenCreateModal = () => {
    showCreateModal = true;
  };

  const handleCloseCreateModal = () => {
    if (!isSaving) {
      showCreateModal = false;
    }
  };

  const handleCloseDetail = () => {
    if (!isSaving) {
      selectedServiceIndex = null;
      showDeleteConfirm = false;
    }
  };

  const handleDeleteFromDetail = () => {
    if (isSaving) return;
    if (showDeleteConfirm) {
      if (selectedService) {
        handleDeleteService(selectedService.name);
      }
    } else {
      showDeleteConfirm = true;
      setTimeout(() => {
        showDeleteConfirm = false;
      }, 3000);
    }
  };

  const handleCancelDelete = () => {
    showDeleteConfirm = false;
  };

  const handleAddUpstream = () => {
    if (selectedServiceIndex === null) return;
    addServiceUpstream(selectedServiceIndex);
  };

  const handleRemoveUpstream = (upstreamIndex: number) => {
    if (selectedServiceIndex === null) return;
    removeServiceUpstream(selectedServiceIndex, upstreamIndex);
  };

  const toggleAdvancedUpstream = (idx: number) => {
    advancedUpstreamExpanded = { ...advancedUpstreamExpanded, [idx]: !advancedUpstreamExpanded[idx] };
  };

  const getDeleteWarningText = (): string => {
    if (!selectedService) return '';
    const routeCount = countRoutesForService(selectedService.name);
    if (routeCount > 0) {
      return `Warning: ${routeCount} route(s) reference this service and will be removed.`;
    }
    return '';
  };

  // Existing service names for modal validation
  $: existingServiceNames = services.map(s => s.name);
</script>

<AppLayout title="Services" subtitle="Manage backend service targets and their upstreams">
  <svelte:fragment slot="header-actions">
    <button
      class="mr-2 rounded-md border border-slate-600 bg-slate-800 px-3 py-1.5 text-xs font-medium text-slate-300 transition-colors hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed"
      on:click={refreshConfig}
      disabled={isLoading || isSaving}
    >
      {#if isLoading}
        <span class="inline-block animate-spin mr-1">⟳</span>
        Loading...
      {:else}
        ⟳ Refresh
      {/if}
    </button>
    <button
      class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
      on:click={handleOpenCreateModal}
      disabled={isSaving}
    >
      + Add Service
    </button>
  </svelte:fragment>

  <!-- Error Notification -->
  {#if errorMessage}
    <div class="fixed top-4 right-4 z-50 max-w-md rounded-lg border border-rose-400/40 bg-rose-500/10 px-4 py-3 shadow-lg backdrop-blur">
      <div class="flex items-start gap-3">
        <span class="text-rose-300 text-lg">✕</span>
        <div class="flex-1">
          <p class="text-sm font-medium text-rose-200">Error</p>
          <p class="mt-1 text-xs text-rose-300/80">{errorMessage}</p>
        </div>
        <button
          class="text-rose-400 hover:text-rose-200 transition-colors"
          on:click={clearError}
        >
          ✕
        </button>
      </div>
    </div>
  {/if}

  <!-- Create Service Modal -->
  {#if showCreateModal}
    <ServiceFormModal
      service={null}
      existingNames={existingServiceNames}
      on:save={(e) => handleCreateService(e.detail)}
      on:cancel={handleCloseCreateModal}
    />
  {/if}

  <div class="p-6">
    {#if isDetailView && selectedService}
      <!-- ================================================================= -->
      <!-- Detail/Edit View -->
      <!-- ================================================================= -->
      <div class="mx-auto max-w-4xl space-y-6">
        <!-- Back Button -->
        <button
          class="inline-flex items-center gap-2 rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100 disabled:opacity-50 disabled:cursor-not-allowed"
          on:click={handleCloseDetail}
          disabled={isSaving}
        >
          <span class="text-base">←</span>
          Back to Services
        </button>

        <!-- Service Header -->
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-xl font-bold text-slate-100">
              {selectedService.name || 'Unnamed Service'}
            </h2>
            <p class="mt-1 text-sm text-slate-400">
              Service #{(selectedServiceIndex ?? 0) + 1}
              <span class="ml-2 rounded-full border border-cyan-400/40 bg-cyan-500/10 px-2 py-0.5 text-xs font-semibold text-cyan-200">
                {formatLbStrategy(selectedService.lb)}
              </span>
            </p>
          </div>

          <!-- Action Buttons -->
          <div class="flex items-center gap-2">
            <!-- Save Button -->
            <button
              class="rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-medium text-emerald-200 transition-colors hover:bg-emerald-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
              on:click={handleSaveService}
              disabled={isSaving}
            >
              {#if isSaving}
                <span class="inline-block animate-spin mr-1">⟳</span>
                Saving...
              {:else}
                Save Changes
              {/if}
            </button>
            <button
              class="rounded-lg border border-rose-400/40 bg-rose-500/10 px-3 py-2 text-sm font-medium text-rose-200 transition-colors hover:bg-rose-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
              on:click={handleDeleteFromDetail}
              disabled={isSaving}
            >
              {#if showDeleteConfirm}
                {#if isSaving}
                  <span class="inline-block animate-spin mr-1">⟳</span>
                  Deleting...
                {:else}
                  Confirm Delete?
                {/if}
              {:else}
                Delete Service
              {/if}
            </button>
            {#if showDeleteConfirm && !isSaving}
              <button
                class="rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-700"
                on:click={handleCancelDelete}
              >
                Cancel
              </button>
            {/if}
          </div>
        </div>

        <!-- Delete Warning -->
        {#if showDeleteConfirm}
          {#if getDeleteWarningText()}
            <div class="rounded-lg border border-amber-400/40 bg-amber-500/10 px-4 py-3 text-sm font-medium text-amber-200">
              <span class="mr-2">⚠</span>
              {getDeleteWarningText()}
            </div>
          {/if}
        {/if}

        <!-- ============================================================= -->
        <!-- Section 1: Service Configuration -->
        <!-- ============================================================= -->
        <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
          <div class="flex items-center gap-3 border-b border-slate-700/80 px-5 py-4">
            <div class="h-8 w-1 rounded-full bg-cyan-400" />
            <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
              Service Configuration
            </h3>
          </div>

          <div class="grid gap-5 p-5 md:grid-cols-2">
            <!-- Name -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Name</span>
              <input
                type="text"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                value={selectedService.name}
                placeholder="e.g., api-backend"
                on:input={(e) => updateServiceLocal('name', inputValue(e))}
                disabled={isSaving}
              />
            </label>

            <!-- LB Strategy -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Load Balancing Strategy</span>
              <select
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                value={selectedService.lb}
                on:change={updateServiceLb}
                disabled={isSaving}
              >
                {#each lbOptions as option}
                  <option value={option}>{formatLbStrategy(option)}</option>
                {/each}
              </select>
            </label>

            <!-- Max Retries -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Max Retries</span>
              <input
                type="number"
                min="0"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                value={selectedService.max_retries}
                on:input={(e) => updateServiceLocal('max_retries', numberValue(e))}
                disabled={isSaving}
              />
            </label>

            <!-- Retry Backoff -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Retry Backoff <span class="text-slate-500">(ms)</span></span>
              <input
                type="number"
                min="0"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                value={selectedService.retry_backoff_ms}
                on:input={(e) => updateServiceLocal('retry_backoff_ms', numberValue(e))}
                disabled={isSaving}
              />
            </label>
          </div>
        </section>

        <!-- ============================================================= -->
        <!-- Section 2: Circuit Breaker -->
        <!-- ============================================================= -->
        <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
          <button
            class="flex w-full items-center justify-between px-5 py-4 text-left transition-colors hover:bg-slate-800/50 disabled:cursor-not-allowed disabled:opacity-50"
            on:click={() => circuitBreakerExpanded = !circuitBreakerExpanded}
            disabled={isSaving}
          >
            <div class="flex items-center gap-3">
              <div class="h-8 w-1 rounded-full bg-amber-400" />
              <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
                Circuit Breaker
              </h3>
              {#if selectedService.circuit_breaker.enabled}
                <span class="rounded-full border border-amber-400/40 bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold text-amber-200">
                  ON
                </span>
              {:else}
                <span class="rounded-full border border-slate-500 bg-slate-800 px-2 py-0.5 text-[10px] font-semibold text-slate-400">
                  OFF
                </span>
              {/if}
            </div>
            <span class={collapseIconClass(circuitBreakerExpanded)}>
              ▼
            </span>
          </button>

          {#if circuitBreakerExpanded}
            <div class="border-t border-slate-700/80 p-5">
              <!-- Enabled Toggle -->
              <div class="mb-5">
                <label class="flex items-center justify-between">
                  <div>
                    <span class="text-sm font-medium text-slate-300">Enable Circuit Breaker</span>
                    <p class="mt-0.5 text-xs text-slate-500">Automatically trip when upstream failures exceed threshold</p>
                  </div>
                  <label class="relative inline-flex cursor-pointer items-center">
                    <input
                      type="checkbox"
                      class="peer sr-only"
                      checked={selectedService.circuit_breaker.enabled}
                      on:change={(e) => updateCircuitBreaker('enabled', checkedValue(e))}
                      disabled={isSaving}
                    />
                    <div class="h-6 w-11 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-amber-600 peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                  </label>
                </label>
              </div>

              {#if selectedService.circuit_breaker.enabled}
                <div class="grid gap-5 md:grid-cols-2">
                  <!-- Consecutive Failures -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-slate-300">Consecutive Failures</span>
                    <p class="text-xs text-slate-500">Number of failures before tripping the breaker</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-amber-500 focus:outline-none focus:ring-1 focus:ring-amber-500/30 disabled:opacity-50"
                      value={selectedService.circuit_breaker.consecutive_failures}
                      on:input={(e) => updateCircuitBreaker('consecutive_failures', numberValue(e, 1))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- Open Duration -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-slate-300">Open Duration <span class="text-slate-500">(ms)</span></span>
                    <p class="text-xs text-slate-500">How long the breaker stays open before retrying</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-amber-500 focus:outline-none focus:ring-1 focus:ring-amber-500/30 disabled:opacity-50"
                      value={selectedService.circuit_breaker.open_ms}
                      on:input={(e) => updateCircuitBreaker('open_ms', numberValue(e, 1))}
                      disabled={isSaving}
                    />
                  </label>
                </div>
              {/if}
            </div>
          {/if}
        </section>

        <!-- ============================================================= -->
        <!-- Section 3: Upstreams -->
        <!-- ============================================================= -->
        <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
          <div class="flex items-center justify-between border-b border-slate-700/80 px-5 py-4">
            <div class="flex items-center gap-3">
              <div class="h-8 w-1 rounded-full bg-emerald-400" />
              <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
                Upstreams
              </h3>
              <span class="rounded-full border border-slate-600 bg-slate-800 px-2 py-0.5 text-xs font-medium text-slate-400">
                {selectedService.upstreams.length}
              </span>
            </div>
            <button
              class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-200 transition-colors hover:bg-emerald-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
              on:click={handleAddUpstream}
              disabled={isSaving}
            >
              <span class="text-sm">+</span>
              <span class="ml-1">Add Upstream</span>
            </button>
          </div>

          <div class="divide-y divide-slate-700/60">
            {#each selectedService.upstreams as upstream, upstreamIndex}
              <div class="p-5">
                <div class="mb-4 flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <span class="flex h-6 w-6 items-center justify-center rounded-md bg-slate-800 text-xs font-bold text-slate-400">
                      {upstreamIndex + 1}
                    </span>
                    <span class="text-sm font-medium text-slate-300">
                      {upstream.addr || 'New Upstream'}
                    </span>
                    {#if upstream.tls}
                      <span class="rounded border border-violet-400/40 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-violet-200">
                        TLS
                      </span>
                    {/if}
                  </div>
                  {#if selectedService.upstreams.length > 1}
                    <button
                      class="rounded-md border border-rose-400/30 bg-rose-500/10 px-2 py-1 text-xs text-rose-300 transition-colors hover:bg-rose-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
                      on:click={() => handleRemoveUpstream(upstreamIndex)}
                      disabled={isSaving}
                    >
                      Remove
                    </button>
                  {/if}
                </div>

                <div class="grid gap-4 md:grid-cols-3">
                  <!-- Address -->
                  <label class="space-y-1.5 md:col-span-2">
                    <span class="text-xs font-medium text-slate-400">Address</span>
                    <input
                      type="text"
                      class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                      value={upstream.addr}
                      placeholder="host:port"
                      on:input={(e) => updateUpstream(upstreamIndex, 'addr', inputValue(e))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- Weight -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-slate-400">Weight <span class="text-slate-600">(1-256)</span></span>
                    <input
                      type="number"
                      min="1"
                      max="256"
                      class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                      value={upstream.weight}
                      on:input={(e) => updateUpstream(upstreamIndex, 'weight', numberValue(e, 1))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- SNI -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-slate-400">SNI <span class="text-slate-600">(optional)</span></span>
                    <input
                      type="text"
                      class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:opacity-50"
                      value={upstream.sni}
                      placeholder="server name"
                      on:input={(e) => updateUpstream(upstreamIndex, 'sni', inputValue(e))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- TLS Toggle -->
                  <div class="flex items-end pb-0.5">
                    <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2">
                      <span class="text-xs font-medium text-slate-400">TLS</span>
                      <label class="relative inline-flex cursor-pointer items-center">
                        <input
                          type="checkbox"
                          class="peer sr-only"
                          checked={upstream.tls}
                          on:change={(e) => updateUpstream(upstreamIndex, 'tls', checkedValue(e))}
                          disabled={isSaving}
                        />
                        <div class="h-5 w-9 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                      </label>
                    </label>
                  </div>

                  <!-- Verify Cert Toggle -->
                  <div class="flex items-end pb-0.5">
                    <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2">
                      <span class="text-xs font-medium text-slate-400">Verify Cert</span>
                      <label class="relative inline-flex cursor-pointer items-center">
                        <input
                          type="checkbox"
                          class="peer sr-only"
                          checked={upstream.verify_cert}
                          on:change={(e) => updateUpstream(upstreamIndex, 'verify_cert', checkedValue(e))}
                          disabled={isSaving}
                        />
                        <div class="h-5 w-9 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                      </label>
                    </label>
                  </div>
                </div>

                <!-- Verify Hostname Toggle -->
                <div class="mt-3">
                  <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2">
                    <span class="text-xs font-medium text-slate-400">Verify Hostname</span>
                    <label class="relative inline-flex cursor-pointer items-center">
                      <input
                        type="checkbox"
                        class="peer sr-only"
                        checked={upstream.verify_hostname}
                        on:change={(e) => updateUpstream(upstreamIndex, 'verify_hostname', checkedValue(e))}
                        disabled={isSaving}
                      />
                      <div class="h-5 w-9 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                    </label>
                  </label>
                </div>

                <!-- Advanced Settings -->
                <div class="mt-4">
                  <button
                    class="flex w-full items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-3 py-2 text-xs text-slate-400 transition-colors hover:bg-slate-800/50 disabled:opacity-50 disabled:cursor-not-allowed"
                    on:click={() => toggleAdvancedUpstream(upstreamIndex)}
                    disabled={isSaving}
                  >
                    <span class="text-xs font-medium text-slate-400">Advanced Timeout Settings</span>
                    <span class={collapseIconClass(advancedUpstreamExpanded[upstreamIndex] ?? false)}>
                      ▼
                    </span>
                  </button>

                  {#if advancedUpstreamExpanded[upstreamIndex]}
                    <div class="mt-2 grid gap-3 rounded-lg border border-slate-700 bg-slate-950/50 p-3 sm:grid-cols-2 lg:grid-cols-3">
                      <!-- Connect Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-slate-500">Connect Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-xs tabular-nums text-slate-200 placeholder:text-slate-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50"
                          value={upstream.connect_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'connect_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Total Connect Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-slate-500">Total Connect Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-xs tabular-nums text-slate-200 placeholder:text-slate-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50"
                          value={upstream.total_connect_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'total_connect_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Read Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-slate-500">Read Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-xs tabular-nums text-slate-200 placeholder:text-slate-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50"
                          value={upstream.read_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'read_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Write Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-slate-500">Write Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-xs tabular-nums text-slate-200 placeholder:text-slate-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50"
                          value={upstream.write_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'write_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Idle Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-slate-500">Idle Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-xs tabular-nums text-slate-200 placeholder:text-slate-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50"
                          value={upstream.idle_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'idle_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>
                    </div>
                  {/if}
                </div>
              </div>
            {/each}

            {#if selectedService.upstreams.length === 0}
              <div class="px-5 py-10 text-center">
                <p class="text-sm text-slate-500">No upstreams configured</p>
                <button
                  class="mt-2 text-sm text-cyan-400 hover:text-cyan-300 disabled:opacity-50 disabled:cursor-not-allowed"
                  on:click={handleAddUpstream}
                  disabled={isSaving}
                >
                  + Add an upstream
                </button>
              </div>
            {/if}
          </div>
        </section>
      </div>
    {:else}
      <!-- ================================================================= -->
      <!-- List View -->
      <!-- ================================================================= -->
      <div class="space-y-4">
        <!-- Search Bar -->
        <div class="flex items-center gap-4">
          <div class="relative flex-1">
            <span class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-500">
              🔍
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-slate-700 bg-slate-950/70 py-2.5 pl-10 pr-4 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30"
              placeholder="Search services by name or load balancing strategy..."
              value={searchQuery}
              on:input={handleSearch}
            />
          </div>
          <div class="flex items-center gap-2 text-sm text-slate-400">
            <span class="rounded-lg border border-slate-700 bg-slate-800/60 px-3 py-2 tabular-nums">
              {filteredServices.length} / {services.length}
            </span>
          </div>
        </div>

        <!-- Services Table -->
        <div class="overflow-hidden rounded-xl border border-slate-700 bg-slate-950/70">
          {#if filteredServices.length === 0}
            <div class="flex flex-col items-center justify-center px-6 py-20">
              <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl border border-slate-700 bg-slate-800/60">
                <span class="text-3xl text-slate-500">⚡</span>
              </div>
              {#if searchQuery}
                <h3 class="text-base font-semibold text-slate-300">No matching services</h3>
                <p class="mt-1 text-sm text-slate-500">
                  No services match "{searchQuery}". Try a different search term or
                  <button
                    class="ml-1 text-cyan-400 hover:text-cyan-300"
                    on:click={() => searchQuery = ''}
                  >
                    clear the search
                  </button>
                </p>
              {:else}
                <h3 class="text-base font-semibold text-slate-300">No services configured</h3>
                <p class="mt-1 text-sm text-slate-500">
                  Get started by adding your first service.
                </p>
                <button
                  class="mt-4 rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-medium text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
                  on:click={handleOpenCreateModal}
                  disabled={isSaving}
                >
                  <span class="mr-1">+</span>
                  Add Service
                </button>
              {/if}
            </div>
          {:else}
            <div class="overflow-x-auto">
              <table class="min-w-full divide-y divide-slate-800 text-sm">
                <thead class="bg-slate-900 text-slate-300">
                  <tr>
                    <th class="px-4 py-3 text-left font-semibold">Name</th>
                    <th class="px-4 py-3 text-left font-semibold">Load Balancing</th>
                    <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
                    <th class="px-4 py-3 text-left font-semibold">Circuit Breaker</th>
                    <th class="px-4 py-3 text-right font-semibold">Actions</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-slate-800">
                  {#each filteredServices as { service, index } (index)}
                    <tr
                      class={rowClass(index)}
                      on:click={() => handleRowClick(index)}
                    >
                      <td class="px-4 py-3">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-slate-100">{service.name || 'Unnamed'}</span>
                          {#if countRoutesForService(service.name) > 0}
                            <span class="rounded-full border border-cyan-400/40 bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-cyan-200">
                              {countRoutesForService(service.name)} route(s)
                            </span>
                          {/if}
                        </div>
                      </td>
                      <td class="px-4 py-3 text-slate-300">
                        {formatLbStrategy(service.lb)}
                      </td>
                      <td class="px-4 py-3 text-slate-400">
                        {service.upstreams.length}
                      </td>
                      <td class="px-4 py-3">
                        <span class={cbBadgeClass(service.circuit_breaker.enabled)}>
                          {service.circuit_breaker.enabled ? 'ON' : 'OFF'}
                        </span>
                      </td>
                      <td class="px-4 py-3">
                        <div class="flex items-center justify-end gap-1.5">
                          <button
                            class="rounded-md border border-slate-600 bg-slate-800 px-2 py-1 text-xs text-slate-300 transition-colors hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleViewService(index)}
                            disabled={isSaving}
                            title="View"
                          >
                            👁
                          </button>
                          <button
                            class="rounded-md border border-cyan-400/30 bg-cyan-500/10 px-2 py-1 text-xs text-cyan-300 transition-colors hover:bg-cyan-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleEditService(index)}
                            disabled={isSaving}
                            title="Edit"
                          >
                            ✏
                          </button>
                          <button
                            class="rounded-md border border-slate-600 bg-slate-800 px-2 py-1 text-xs text-slate-300 transition-colors hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleDuplicateService(index)}
                            disabled={isSaving}
                            title="Duplicate"
                          >
                            📋
                          </button>
                          <button
                            class="rounded-md border border-rose-400/30 bg-rose-500/10 px-2 py-1 text-xs text-rose-300 transition-colors hover:bg-rose-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleDeleteFromList(index)}
                            disabled={isSaving}
                            title="Delete"
                          >
                            🗑
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>

        {#if services.length > 0 && filteredServices.length === 0 && searchQuery}
          <div class="text-center text-sm text-slate-500">
            <span class="text-slate-400">{services.length} services total</span>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</AppLayout>