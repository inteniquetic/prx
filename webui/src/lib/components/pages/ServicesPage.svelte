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
  /** Which service the URL is pointing at — `/services/:name`. */
  export let selectedServiceName: string | null = null;

  // ---------------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------------

  const dispatch = createEventDispatcher<{
    navigate: NavPage;
    /** The service now on screen, so the shell can put it in the address bar. */
    select: string | null;
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

  // Same deal as Routes: the URL owns the selection, the page reports changes.
  $: {
    const found = selectedServiceName
      ? services.findIndex((entry) => entry.name === selectedServiceName)
      : -1;
    const next = found >= 0 ? found : null;
    if (next !== selectedServiceIndex) {
      selectedServiceIndex = next;
    }
  }

  // The page asks for a selection instead of assigning one, so the URL stays
  // the single source of truth even before the config has loaded.
  const selectService = (index: number | null) => {
    dispatch('select', index === null ? null : services[index]?.name ?? null);
  };

  const selectServiceByName = (name: string | null) => {
    dispatch('select', name);
  };

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
      ? 'rounded-full border border-warning/40 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold text-warning'
      : 'rounded-full border border-border bg-muted px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground';

  const rowClass = (index: number): string =>
    selectedServiceIndex === index
      ? 'cursor-pointer transition-colors bg-primary/10'
      : 'cursor-pointer transition-colors hover:bg-card/70';

  const collapseIconClass = (expanded: boolean): string =>
    expanded ? 'text-muted-foreground transition-transform rotate-180' : 'text-muted-foreground transition-transform';

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

      // Re-select the service if we were in detail view; the name survives a
      // reorder, the index does not.
      if (selectedService !== null) {
        const stillThere = newConfig.services.some(
          (s: ServiceConfig) => s.name === selectedService.name
        );
        selectServiceByName(stillThere ? selectedService.name : null);
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
      // Open the service that was just created.
      if (config.services.some((s) => s.name === serviceData.name)) {
        selectServiceByName(serviceData.name);
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
      selectService(null);
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
    selectService(index);
  };

  const handleViewService = (index: number) => {
    selectService(index);
  };

  const handleEditService = (index: number) => {
    selectService(index);
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
      selectService(null);
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
      selectService(null);
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
      class="mr-2 rounded-md border border-border bg-muted px-3 py-1.5 text-xs font-medium text-foreground/80 transition-colors hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
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
      class="rounded-md border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary transition-colors hover:bg-primary/20 disabled:opacity-50 disabled:cursor-not-allowed"
      on:click={handleOpenCreateModal}
      disabled={isSaving}
    >
      + Add Service
    </button>
  </svelte:fragment>

  <!-- Error Notification -->
  {#if errorMessage}
    <div class="fixed top-4 right-4 z-50 max-w-md rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 shadow-lg backdrop-blur">
      <div class="flex items-start gap-3">
        <span class="text-destructive text-lg">✕</span>
        <div class="flex-1">
          <p class="text-sm font-medium text-destructive">Error</p>
          <p class="mt-1 text-xs text-destructive/80">{errorMessage}</p>
        </div>
        <button
          class="text-destructive hover:text-destructive transition-colors"
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
          class="inline-flex items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed"
          on:click={handleCloseDetail}
          disabled={isSaving}
        >
          <span class="text-base">←</span>
          Back to Services
        </button>

        <!-- Service Header -->
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-xl font-bold text-foreground">
              {selectedService.name || 'Unnamed Service'}
            </h2>
            <p class="mt-1 text-sm text-muted-foreground">
              Service #{(selectedServiceIndex ?? 0) + 1}
              <span class="ml-2 rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-xs font-semibold text-primary">
                {formatLbStrategy(selectedService.lb)}
              </span>
            </p>
          </div>

          <!-- Action Buttons -->
          <div class="flex items-center gap-2">
            <!-- Save Button -->
            <button
              class="rounded-lg border border-success/40 bg-success/10 px-4 py-2 text-sm font-medium text-success transition-colors hover:bg-success/20 disabled:opacity-50 disabled:cursor-not-allowed"
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
              class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm font-medium text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50 disabled:cursor-not-allowed"
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
                class="rounded-lg border border-border bg-muted px-3 py-2 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted"
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
            <div class="rounded-lg border border-warning/40 bg-warning/10 px-4 py-3 text-sm font-medium text-warning">
              <span class="mr-2">⚠</span>
              {getDeleteWarningText()}
            </div>
          {/if}
        {/if}

        <!-- ============================================================= -->
        <!-- Section 1: Service Configuration -->
        <!-- ============================================================= -->
        <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
          <div class="flex items-center gap-3 border-b border-border/80 px-5 py-4">
            <div class="h-8 w-1 rounded-full bg-primary" ></div>
            <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
              Service Configuration
            </h3>
          </div>

          <div class="grid gap-5 p-5 md:grid-cols-2">
            <!-- Name -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-foreground/80">Name</span>
              <input
                type="text"
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
                value={selectedService.name}
                placeholder="e.g., api-backend"
                on:input={(e) => updateServiceLocal('name', inputValue(e))}
                disabled={isSaving}
              />
            </label>

            <!-- LB Strategy -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-foreground/80">Load Balancing Strategy</span>
              <select
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
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
              <span class="text-sm font-medium text-foreground/80">Max Retries</span>
              <input
                type="number"
                min="0"
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
                value={selectedService.max_retries}
                on:input={(e) => updateServiceLocal('max_retries', numberValue(e))}
                disabled={isSaving}
              />
            </label>

            <!-- Retry Backoff -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-foreground/80">Retry Backoff <span class="text-muted-foreground">(ms)</span></span>
              <input
                type="number"
                min="0"
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
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
        <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
          <button
            class="flex w-full items-center justify-between px-5 py-4 text-left transition-colors hover:bg-muted/50 disabled:cursor-not-allowed disabled:opacity-50"
            on:click={() => circuitBreakerExpanded = !circuitBreakerExpanded}
            disabled={isSaving}
          >
            <div class="flex items-center gap-3">
              <div class="h-8 w-1 rounded-full bg-warning" ></div>
              <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
                Circuit Breaker
              </h3>
              {#if selectedService.circuit_breaker.enabled}
                <span class="rounded-full border border-warning/40 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold text-warning">
                  ON
                </span>
              {:else}
                <span class="rounded-full border border-border bg-muted px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                  OFF
                </span>
              {/if}
            </div>
            <span class={collapseIconClass(circuitBreakerExpanded)}>
              ▼
            </span>
          </button>

          {#if circuitBreakerExpanded}
            <div class="border-t border-border/80 p-5">
              <!-- Enabled Toggle -->
              <div class="mb-5">
                <label class="flex items-center justify-between">
                  <div>
                    <span class="text-sm font-medium text-foreground/80">Enable Circuit Breaker</span>
                    <p class="mt-0.5 text-xs text-muted-foreground">Automatically trip when upstream failures exceed threshold</p>
                  </div>
                  <label class="relative inline-flex cursor-pointer items-center">
                    <input
                      type="checkbox"
                      class="peer sr-only"
                      checked={selectedService.circuit_breaker.enabled}
                      on:change={(e) => updateCircuitBreaker('enabled', checkedValue(e))}
                      disabled={isSaving}
                    />
                    <div class="h-6 w-11 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-warning peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                  </label>
                </label>
              </div>

              {#if selectedService.circuit_breaker.enabled}
                <div class="grid gap-5 md:grid-cols-2">
                  <!-- Consecutive Failures -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-foreground/80">Consecutive Failures</span>
                    <p class="text-xs text-muted-foreground">Number of failures before tripping the breaker</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-warning focus:outline-none focus:ring-1 focus:ring-warning/30 disabled:opacity-50"
                      value={selectedService.circuit_breaker.consecutive_failures}
                      on:input={(e) => updateCircuitBreaker('consecutive_failures', numberValue(e, 1))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- Open Duration -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-foreground/80">Open Duration <span class="text-muted-foreground">(ms)</span></span>
                    <p class="text-xs text-muted-foreground">How long the breaker stays open before retrying</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-warning focus:outline-none focus:ring-1 focus:ring-warning/30 disabled:opacity-50"
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
        <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
          <div class="flex items-center justify-between border-b border-border/80 px-5 py-4">
            <div class="flex items-center gap-3">
              <div class="h-8 w-1 rounded-full bg-success" ></div>
              <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
                Upstreams
              </h3>
              <span class="rounded-full border border-border bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                {selectedService.upstreams.length}
              </span>
            </div>
            <button
              class="rounded-md border border-success/40 bg-success/10 px-2.5 py-1 text-xs font-medium text-success transition-colors hover:bg-success/20 disabled:opacity-50 disabled:cursor-not-allowed"
              on:click={handleAddUpstream}
              disabled={isSaving}
            >
              <span class="text-sm">+</span>
              <span class="ml-1">Add Upstream</span>
            </button>
          </div>

          <div class="divide-y divide-border/60">
            {#each selectedService.upstreams as upstream, upstreamIndex}
              <div class="p-5">
                <div class="mb-4 flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <span class="flex h-6 w-6 items-center justify-center rounded-md bg-muted text-xs font-bold text-muted-foreground">
                      {upstreamIndex + 1}
                    </span>
                    <span class="text-sm font-medium text-foreground/80">
                      {upstream.addr || 'New Upstream'}
                    </span>
                    {#if upstream.tls}
                      <span class="rounded border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                        TLS
                      </span>
                    {/if}
                  </div>
                  {#if selectedService.upstreams.length > 1}
                    <button
                      class="rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50 disabled:cursor-not-allowed"
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
                    <span class="text-xs font-medium text-muted-foreground">Address</span>
                    <input
                      type="text"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
                      value={upstream.addr}
                      placeholder="host:port"
                      on:input={(e) => updateUpstream(upstreamIndex, 'addr', inputValue(e))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- Weight -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-muted-foreground">Weight <span class="text-muted-foreground/70">(1-256)</span></span>
                    <input
                      type="number"
                      min="1"
                      max="256"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
                      value={upstream.weight}
                      on:input={(e) => updateUpstream(upstreamIndex, 'weight', numberValue(e, 1))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- SNI -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-muted-foreground">SNI <span class="text-muted-foreground/70">(optional)</span></span>
                    <input
                      type="text"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:opacity-50"
                      value={upstream.sni}
                      placeholder="server name"
                      on:input={(e) => updateUpstream(upstreamIndex, 'sni', inputValue(e))}
                      disabled={isSaving}
                    />
                  </label>

                  <!-- TLS Toggle -->
                  <div class="flex items-end pb-0.5">
                    <label class="flex items-center justify-between gap-3 rounded-lg border border-border bg-background/70 px-3 py-2">
                      <span class="text-xs font-medium text-muted-foreground">TLS</span>
                      <label class="relative inline-flex cursor-pointer items-center">
                        <input
                          type="checkbox"
                          class="peer sr-only"
                          checked={upstream.tls}
                          on:change={(e) => updateUpstream(upstreamIndex, 'tls', checkedValue(e))}
                          disabled={isSaving}
                        />
                        <div class="h-5 w-9 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-primary peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                      </label>
                    </label>
                  </div>

                  <!-- Verify Cert Toggle -->
                  <div class="flex items-end pb-0.5">
                    <label class="flex items-center justify-between gap-3 rounded-lg border border-border bg-background/70 px-3 py-2">
                      <span class="text-xs font-medium text-muted-foreground">Verify Cert</span>
                      <label class="relative inline-flex cursor-pointer items-center">
                        <input
                          type="checkbox"
                          class="peer sr-only"
                          checked={upstream.verify_cert}
                          on:change={(e) => updateUpstream(upstreamIndex, 'verify_cert', checkedValue(e))}
                          disabled={isSaving}
                        />
                        <div class="h-5 w-9 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-primary peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                      </label>
                    </label>
                  </div>
                </div>

                <!-- Verify Hostname Toggle -->
                <div class="mt-3">
                  <label class="flex items-center justify-between gap-3 rounded-lg border border-border bg-background/70 px-3 py-2">
                    <span class="text-xs font-medium text-muted-foreground">Verify Hostname</span>
                    <label class="relative inline-flex cursor-pointer items-center">
                      <input
                        type="checkbox"
                        class="peer sr-only"
                        checked={upstream.verify_hostname}
                        on:change={(e) => updateUpstream(upstreamIndex, 'verify_hostname', checkedValue(e))}
                        disabled={isSaving}
                      />
                      <div class="h-5 w-9 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-primary peer-checked:after:translate-x-full peer-checked:after:bg-white disabled:opacity-50"></div>
                    </label>
                  </label>
                </div>

                <!-- Advanced Settings -->
                <div class="mt-4">
                  <button
                    class="flex w-full items-center justify-between rounded-lg border border-border bg-card/50 px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-muted/50 disabled:opacity-50 disabled:cursor-not-allowed"
                    on:click={() => toggleAdvancedUpstream(upstreamIndex)}
                    disabled={isSaving}
                  >
                    <span class="text-xs font-medium text-muted-foreground">Advanced Timeout Settings</span>
                    <span class={collapseIconClass(advancedUpstreamExpanded[upstreamIndex] ?? false)}>
                      ▼
                    </span>
                  </button>

                  {#if advancedUpstreamExpanded[upstreamIndex]}
                    <div class="mt-2 grid gap-3 rounded-lg border border-border bg-background/50 p-3 sm:grid-cols-2 lg:grid-cols-3">
                      <!-- Connect Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-muted-foreground">Connect Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-border bg-card px-2 py-1.5 text-xs tabular-nums text-foreground placeholder:text-muted-foreground/70 focus:border-primary focus:outline-none disabled:opacity-50"
                          value={upstream.connect_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'connect_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Total Connect Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-muted-foreground">Total Connect Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-border bg-card px-2 py-1.5 text-xs tabular-nums text-foreground placeholder:text-muted-foreground/70 focus:border-primary focus:outline-none disabled:opacity-50"
                          value={upstream.total_connect_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'total_connect_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Read Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-muted-foreground">Read Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-border bg-card px-2 py-1.5 text-xs tabular-nums text-foreground placeholder:text-muted-foreground/70 focus:border-primary focus:outline-none disabled:opacity-50"
                          value={upstream.read_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'read_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Write Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-muted-foreground">Write Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-border bg-card px-2 py-1.5 text-xs tabular-nums text-foreground placeholder:text-muted-foreground/70 focus:border-primary focus:outline-none disabled:opacity-50"
                          value={upstream.write_timeout_ms ?? ''}
                          placeholder="default"
                          on:input={(e) => updateUpstream(upstreamIndex, 'write_timeout_ms', toNullableNumber(inputValue(e)))}
                          disabled={isSaving}
                        />
                      </label>

                      <!-- Idle Timeout -->
                      <label class="space-y-1">
                        <span class="text-[11px] font-medium text-muted-foreground">Idle Timeout (ms)</span>
                        <input
                          type="number"
                          min="0"
                          class="w-full rounded border border-border bg-card px-2 py-1.5 text-xs tabular-nums text-foreground placeholder:text-muted-foreground/70 focus:border-primary focus:outline-none disabled:opacity-50"
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
                <p class="text-sm text-muted-foreground">No upstreams configured</p>
                <button
                  class="mt-2 text-sm text-primary hover:text-primary disabled:opacity-50 disabled:cursor-not-allowed"
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
            <span class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">
              🔍
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-border bg-background/70 py-2.5 pl-10 pr-4 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
              placeholder="Search services by name or load balancing strategy..."
              value={searchQuery}
              on:input={handleSearch}
            />
          </div>
          <div class="flex items-center gap-2 text-sm text-muted-foreground">
            <span class="rounded-lg border border-border bg-muted/60 px-3 py-2 tabular-nums">
              {filteredServices.length} / {services.length}
            </span>
          </div>
        </div>

        <!-- Services Table -->
        <div class="overflow-hidden rounded-xl border border-border bg-background/70">
          {#if filteredServices.length === 0}
            <div class="flex flex-col items-center justify-center px-6 py-20">
              <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl border border-border bg-muted/60">
                <span class="text-3xl text-muted-foreground">⚡</span>
              </div>
              {#if searchQuery}
                <h3 class="text-base font-semibold text-foreground/80">No matching services</h3>
                <p class="mt-1 text-sm text-muted-foreground">
                  No services match "{searchQuery}". Try a different search term or
                  <button
                    class="ml-1 text-primary hover:text-primary"
                    on:click={() => searchQuery = ''}
                  >
                    clear the search
                  </button>
                </p>
              {:else}
                <h3 class="text-base font-semibold text-foreground/80">No services configured</h3>
                <p class="mt-1 text-sm text-muted-foreground">
                  Get started by adding your first service.
                </p>
                <button
                  class="mt-4 rounded-lg border border-primary/40 bg-primary/10 px-4 py-2 text-sm font-medium text-primary transition-colors hover:bg-primary/20 disabled:opacity-50 disabled:cursor-not-allowed"
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
              <table class="min-w-full divide-y divide-border text-sm">
                <thead class="bg-card text-foreground/80">
                  <tr>
                    <th class="px-4 py-3 text-left font-semibold">Name</th>
                    <th class="px-4 py-3 text-left font-semibold">Load Balancing</th>
                    <th class="px-4 py-3 text-left font-semibold">Upstreams</th>
                    <th class="px-4 py-3 text-left font-semibold">Circuit Breaker</th>
                    <th class="px-4 py-3 text-right font-semibold">Actions</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border">
                  {#each filteredServices as { service, index } (index)}
                    <tr
                      class={rowClass(index)}
                      on:click={() => handleRowClick(index)}
                    >
                      <td class="px-4 py-3">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-foreground">{service.name || 'Unnamed'}</span>
                          {#if countRoutesForService(service.name) > 0}
                            <span class="rounded-full border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                              {countRoutesForService(service.name)} route(s)
                            </span>
                          {/if}
                        </div>
                      </td>
                      <td class="px-4 py-3 text-foreground/80">
                        {formatLbStrategy(service.lb)}
                      </td>
                      <td class="px-4 py-3 text-muted-foreground">
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
                            class="rounded-md border border-border bg-muted px-2 py-1 text-xs text-foreground/80 transition-colors hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleViewService(index)}
                            disabled={isSaving}
                            title="View"
                          >
                            👁
                          </button>
                          <button
                            class="rounded-md border border-primary/30 bg-primary/10 px-2 py-1 text-xs text-primary transition-colors hover:bg-primary/20 disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleEditService(index)}
                            disabled={isSaving}
                            title="Edit"
                          >
                            ✏
                          </button>
                          <button
                            class="rounded-md border border-border bg-muted px-2 py-1 text-xs text-foreground/80 transition-colors hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click|stopPropagation={() => handleDuplicateService(index)}
                            disabled={isSaving}
                            title="Duplicate"
                          >
                            📋
                          </button>
                          <button
                            class="rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50 disabled:cursor-not-allowed"
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
          <div class="text-center text-sm text-muted-foreground">
            <span class="text-muted-foreground">{services.length} services total</span>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</AppLayout>