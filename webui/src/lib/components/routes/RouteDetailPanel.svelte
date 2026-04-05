<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { addUpstream, configStore, removeUpstream } from '../../stores/config';
  import type { LbStrategy, RouteConfig } from '../../types/config';

  export let route: RouteConfig;
  export let routeIndex: number;
  export let mode: 'view' | 'edit' = 'edit';

  const dispatch = createEventDispatcher<{
    close: void;
    deleteRoute: number;
  }>();

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let circuitBreakerExpanded = route.circuit_breaker.enabled;
  let advancedUpstreamExpanded: Record<number, boolean> = {};
  let showDeleteConfirm = false;

  $: isViewMode = mode === 'view';

  const lbOptions: LbStrategy[] = ['round_robin', 'random', 'hash'];

  // ---------------------------------------------------------------------------
  // Input helpers
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
  // Update functions (matches RouteCard.svelte patterns)
  // ---------------------------------------------------------------------------

  const updateRoute = <K extends keyof RouteConfig>(key: K, value: RouteConfig[K]) => {
    configStore.update((config) => {
      const target = config.routes[routeIndex];
      if (!target) return config;
      target[key] = value;
      return config;
    });
  };

  const updateRouteLb = (event: Event) => {
    updateRoute('lb', selectValue(event) as LbStrategy);
  };

  const updateCircuitBreaker = (
    key: 'enabled' | 'consecutive_failures' | 'open_ms',
    value: number | boolean
  ) => {
    configStore.update((config) => {
      const routeValue = config.routes[routeIndex];
      if (!routeValue) return config;
      if (key === 'enabled') {
        routeValue.circuit_breaker.enabled = Boolean(value);
        circuitBreakerExpanded = Boolean(value);
      } else if (key === 'consecutive_failures') {
        routeValue.circuit_breaker.consecutive_failures = Math.max(1, Number(value));
      } else {
        routeValue.circuit_breaker.open_ms = Math.max(1, Number(value));
      }
      return config;
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
    configStore.update((config) => {
      const upstream = config.routes[routeIndex]?.upstreams[upstreamIndex];
      if (!upstream) return config;
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
      return config;
    });
  };

  const toggleAdvancedUpstream = (idx: number) => {
    advancedUpstreamExpanded = { ...advancedUpstreamExpanded, [idx]: !advancedUpstreamExpanded[idx] };
  };

  const handleAddUpstream = () => {
    addUpstream(routeIndex);
  };

  const handleRemoveUpstream = (upstreamIndex: number) => {
    removeUpstream(routeIndex, upstreamIndex);
  };

  const handleDelete = () => {
    if (showDeleteConfirm) {
      dispatch('deleteRoute', routeIndex);
      showDeleteConfirm = false;
    } else {
      showDeleteConfirm = true;
      // Auto-dismiss confirmation after 3 seconds
      setTimeout(() => {
        showDeleteConfirm = false;
      }, 3000);
    }
  };

  const handleCancelDelete = () => {
    showDeleteConfirm = false;
  };

  // ---------------------------------------------------------------------------
  // Formatters
  // ---------------------------------------------------------------------------

  const formatLbStrategy = (lb: string): string => {
    switch (lb) {
      case 'round_robin': return 'Round Robin';
      case 'random': return 'Random';
      case 'hash': return 'Hash';
      default: return lb;
    }
  };
</script>

<div class="space-y-6">
  <!-- Back Button -->
  <button
    class="inline-flex items-center gap-2 rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-800 hover:text-slate-100"
    on:click={() => dispatch('close')}
  >
    <span class="text-base">←</span>
    Back to Routes
  </button>

  <!-- Route Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold text-slate-100">
        {route.name || 'Unnamed Route'}
      </h2>
      <p class="mt-1 text-sm text-slate-400">
        Route #{routeIndex + 1} · {isViewMode ? 'View Mode' : 'Edit Mode'}
        {#if route.is_default}
          <span class="ml-2 rounded-full border border-emerald-400/40 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-200">
            Default
          </span>
        {/if}
      </p>
    </div>

    <!-- Action Buttons (Edit mode only) -->
    {#if !isViewMode}
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg border border-rose-400/40 bg-rose-500/10 px-3 py-2 text-sm font-medium text-rose-200 transition-colors hover:bg-rose-500/20"
          on:click={handleDelete}
        >
          {#if showDeleteConfirm}
            Confirm Delete?
          {:else}
            Delete Route
          {/if}
        </button>
        {#if showDeleteConfirm}
          <button
            class="rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-700"
            on:click={handleCancelDelete}
          >
            Cancel
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- ========================================================================= -->
  <!-- Section 1: Route Configuration -->
  <!-- ========================================================================= -->
  <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
    <div class="flex items-center gap-3 border-b border-slate-700/80 px-5 py-4">
      <div class="h-8 w-1 rounded-full bg-cyan-400" />
      <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
        Route Configuration
      </h3>
    </div>

    <div class="grid gap-5 p-5 md:grid-cols-2">
      <!-- Name -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Name</span>
        <input
          type="text"
          class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
          disabled={isViewMode}
          value={route.name}
          placeholder="e.g., api-gateway"
          on:input={(e) => updateRoute('name', inputValue(e))}
        />
      </label>

      <!-- Host -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Host <span class="text-slate-500">(optional)</span></span>
        <input
          type="text"
          class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
          disabled={isViewMode}
          value={route.host}
          placeholder="e.g., api.example.com"
          on:input={(e) => updateRoute('host', inputValue(e))}
        />
      </label>

      <!-- Path Prefix -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Path Prefix</span>
        <input
          type="text"
          class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm font-mono text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
          disabled={isViewMode}
          value={route.path_prefix}
          placeholder="/"
          on:input={(e) => updateRoute('path_prefix', inputValue(e))}
        />
      </label>

      <!-- LB Strategy -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Load Balancing Strategy</span>
        {#if isViewMode}
          <div class="flex h-[42px] items-center rounded-lg border border-slate-600 bg-slate-800/50 px-3 text-sm text-slate-400">
            {formatLbStrategy(route.lb)}
          </div>
        {:else}
          <select
            class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30"
            value={route.lb}
            on:change={updateRouteLb}
          >
            {#each lbOptions as option}
              <option value={option}>{formatLbStrategy(option)}</option>
            {/each}
          </select>
        {/if}
      </label>

      <!-- Max Retries -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Max Retries</span>
        <input
          type="number"
          min="0"
          class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
          disabled={isViewMode}
          value={route.max_retries}
          on:input={(e) => updateRoute('max_retries', numberValue(e))}
        />
      </label>

      <!-- Retry Backoff -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-slate-300">Retry Backoff <span class="text-slate-500">(ms)</span></span>
        <input
          type="number"
          min="0"
          class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
          disabled={isViewMode}
          value={route.retry_backoff_ms}
          on:input={(e) => updateRoute('retry_backoff_ms', numberValue(e))}
        />
      </label>
    </div>

    <!-- Default Route Toggle -->
    <div class="border-t border-slate-700/80 px-5 py-4">
      <label class="flex items-center justify-between">
        <div>
          <span class="text-sm font-medium text-slate-300">Default Route</span>
          <p class="mt-0.5 text-xs text-slate-500">This route will handle requests that don't match any other route</p>
        </div>
        <!-- Toggle Switch -->
        <div class="relative">
          <input
            type="checkbox"
            class="peer sr-only"
            id="is-default-toggle"
            disabled={isViewMode}
            checked={route.is_default}
            on:change={(e) => updateRoute('is_default', checkedValue(e))}
          />
          <label
            for="is-default-toggle"
            class="inline-flex h-6 w-11 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-cyan-500 peer-checked:bg-cyan-500/30 peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
          >
            <span class="ml-0.5 h-5 w-5 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-5 peer-checked:border-cyan-400 peer-checked:bg-cyan-300" />
          </label>
        </div>
      </label>
    </div>
  </section>

  <!-- ========================================================================= -->
  <!-- Section 2: Circuit Breaker -->
  <!-- ========================================================================= -->
  <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
    <button
      class="flex w-full items-center justify-between px-5 py-4 text-left transition-colors hover:bg-slate-800/50"
      on:click={() => circuitBreakerExpanded = !circuitBreakerExpanded}
    >
      <div class="flex items-center gap-3">
        <div class="h-8 w-1 rounded-full bg-amber-400" />
        <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
          Circuit Breaker
        </h3>
        {#if route.circuit_breaker.enabled}
          <span class="rounded-full border border-amber-400/40 bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold text-amber-200">
            ON
          </span>
        {:else}
          <span class="rounded-full border border-slate-500 bg-slate-800 px-2 py-0.5 text-[10px] font-semibold text-slate-400">
            OFF
          </span>
        {/if}
      </div>
      <span class="text-slate-400 transition-transform {circuitBreakerExpanded ? 'rotate-180' : ''}">
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
            <div class="relative">
              <input
                type="checkbox"
                class="peer sr-only"
                id="cb-enabled-toggle"
                disabled={isViewMode}
                checked={route.circuit_breaker.enabled}
                on:change={(e) => updateCircuitBreaker('enabled', checkedValue(e))}
              />
              <label
                for="cb-enabled-toggle"
                class="inline-flex h-6 w-11 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-amber-500 peer-checked:bg-amber-500/30 peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
              >
                <span class="ml-0.5 h-5 w-5 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-5 peer-checked:border-amber-400 peer-checked:bg-amber-300" />
              </label>
            </div>
          </label>
        </div>

        {#if route.circuit_breaker.enabled || !isViewMode}
          <div class="grid gap-5 md:grid-cols-2">
            <!-- Consecutive Failures -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Consecutive Failures</span>
              <p class="text-xs text-slate-500">Number of failures before tripping the breaker</p>
              <input
                type="number"
                min="1"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                disabled={isViewMode}
                value={route.circuit_breaker.consecutive_failures}
                on:input={(e) => updateCircuitBreaker('consecutive_failures', numberValue(e, 1))}
              />
            </label>

            <!-- Open Duration -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-slate-300">Open Duration <span class="text-slate-500">(ms)</span></span>
              <p class="text-xs text-slate-500">How long the breaker stays open before attempting recovery</p>
              <input
                type="number"
                min="1"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                disabled={isViewMode}
                value={route.circuit_breaker.open_ms}
                on:input={(e) => updateCircuitBreaker('open_ms', numberValue(e, 1))}
              />
            </label>
          </div>
        {/if}
      </div>
    {/if}
  </section>

  <!-- ========================================================================= -->
  <!-- Section 3: Upstreams -->
  <!-- ========================================================================= -->
  <section class="rounded-xl border border-slate-700/80 bg-slate-900/80 backdrop-blur">
    <div class="flex items-center justify-between border-b border-slate-700/80 px-5 py-4">
      <div class="flex items-center gap-3">
        <div class="h-8 w-1 rounded-full bg-emerald-400" />
        <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
          Upstreams
        </h3>
        <span class="rounded-full border border-slate-600 bg-slate-800 px-2 py-0.5 text-xs font-medium text-slate-400">
          {route.upstreams.length}
        </span>
      </div>
      {#if !isViewMode}
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
          on:click={handleAddUpstream}
        >
          <span class="text-sm">+</span>
          Add Upstream
        </button>
      {/if}
    </div>

    <div class="divide-y divide-slate-700/60">
      {#each route.upstreams as upstream, upstreamIndex}
        <div class="p-5">
          <!-- Upstream Header -->
          <div class="mb-4 flex items-center justify-between">
            <div class="flex items-center gap-2">
              <span class="flex h-6 w-6 items-center justify-center rounded-md bg-slate-800 text-xs font-bold text-slate-400">
                {upstreamIndex + 1}
              </span>
              <span class="text-sm font-medium text-slate-300">
                {upstream.addr || 'Unconfigured'}
              </span>
              {#if upstream.tls}
                <span class="rounded border border-violet-400/40 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-violet-200">
                  TLS
                </span>
              {/if}
            </div>
            {#if !isViewMode && route.upstreams.length > 1}
              <button
                class="rounded-md border border-rose-400/40 bg-rose-500/10 px-2 py-1 text-xs font-medium text-rose-200 transition-colors hover:bg-rose-500/20"
                on:click={() => handleRemoveUpstream(upstreamIndex)}
              >
                Remove
              </button>
            {/if}
          </div>

          <!-- Upstream Fields -->
          <div class="grid gap-4 md:grid-cols-3">
            <!-- Address -->
            <label class="space-y-1.5 md:col-span-2">
              <span class="text-xs font-medium text-slate-400">Address</span>
              <input
                type="text"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm font-mono text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                disabled={isViewMode}
                value={upstream.addr}
                placeholder="host:port"
                on:input={(e) => updateUpstream(upstreamIndex, 'addr', inputValue(e))}
              />
            </label>

            <!-- Weight -->
            <label class="space-y-1.5">
              <span class="text-xs font-medium text-slate-400">Weight <span class="text-slate-600">(1-256)</span></span>
              <input
                type="number"
                min="1"
                max="256"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm tabular-nums text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                disabled={isViewMode}
                value={upstream.weight}
                on:input={(e) => updateUpstream(upstreamIndex, 'weight', numberValue(e, 1))}
              />
            </label>

            <!-- SNI -->
            <label class="space-y-1.5">
              <span class="text-xs font-medium text-slate-400">SNI <span class="text-slate-600">(TLS)</span></span>
              <input
                type="text"
                class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                disabled={isViewMode}
                value={upstream.sni}
                placeholder="hostname"
                on:input={(e) => updateUpstream(upstreamIndex, 'sni', inputValue(e))}
              />
            </label>

            <!-- TLS Toggle -->
            <div class="flex items-end pb-0.5">
              <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 disabled:cursor-not-allowed disabled:opacity-50">
                <span class="text-xs font-medium text-slate-400">TLS Enabled</span>
                <div class="relative">
                  <input
                    type="checkbox"
                    class="peer sr-only"
                    id="tls-toggle-{upstreamIndex}"
                    disabled={isViewMode}
                    checked={upstream.tls}
                    on:change={(e) => updateUpstream(upstreamIndex, 'tls', checkedValue(e))}
                  />
                  <label
                    for="tls-toggle-{upstreamIndex}"
                    class="inline-flex h-5 w-9 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-cyan-500 peer-checked:bg-cyan-500/30 peer-disabled:cursor-not-allowed"
                  >
                    <span class="ml-0.5 h-4 w-4 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-4 peer-checked:border-cyan-400 peer-checked:bg-cyan-300" />
                  </label>
                </div>
              </label>
            </div>

            <!-- Verify Cert Toggle -->
            <div class="flex items-end pb-0.5">
              <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 disabled:cursor-not-allowed disabled:opacity-50">
                <span class="text-xs font-medium text-slate-400">Verify Cert</span>
                <div class="relative">
                  <input
                    type="checkbox"
                    class="peer sr-only"
                    id="verify-cert-toggle-{upstreamIndex}"
                    disabled={isViewMode}
                    checked={upstream.verify_cert ?? true}
                    on:change={(e) => updateUpstream(upstreamIndex, 'verify_cert', checkedValue(e))}
                  />
                  <label
                    for="verify-cert-toggle-{upstreamIndex}"
                    class="inline-flex h-5 w-9 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-cyan-500 peer-checked:bg-cyan-500/30 peer-disabled:cursor-not-allowed"
                  >
                    <span class="ml-0.5 h-4 w-4 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-4 peer-checked:border-cyan-400 peer-checked:bg-cyan-300" />
                  </label>
                </div>
              </label>
            </div>
          </div>

          <!-- Verify Hostname (on its own row for clarity) -->
          <div class="mt-3">
            <label class="flex items-center justify-between gap-3 rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2 disabled:cursor-not-allowed disabled:opacity-50">
              <span class="text-xs font-medium text-slate-400">Verify Hostname</span>
              <div class="relative">
                <input
                  type="checkbox"
                  class="peer sr-only"
                  id="verify-hostname-toggle-{upstreamIndex}"
                  disabled={isViewMode}
                  checked={upstream.verify_hostname ?? true}
                  on:change={(e) => updateUpstream(upstreamIndex, 'verify_hostname', checkedValue(e))}
                />
                <label
                  for="verify-hostname-toggle-{upstreamIndex}"
                  class="inline-flex h-5 w-9 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-cyan-500 peer-checked:bg-cyan-500/30 peer-disabled:cursor-not-allowed"
                >
                  <span class="ml-0.5 h-4 w-4 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-4 peer-checked:border-cyan-400 peer-checked:bg-cyan-300" />
                </label>
              </div>
            </label>
          </div>

          <!-- Advanced Timeout Section (Collapsible) -->
          <div class="mt-4">
            <button
              class="flex w-full items-center justify-between rounded-lg border border-slate-700 bg-slate-800/40 px-3 py-2 text-left transition-colors hover:bg-slate-800/60"
              on:click={() => toggleAdvancedUpstream(upstreamIndex)}
            >
              <span class="text-xs font-medium text-slate-400">
                ⚙ Advanced Timeouts
              </span>
              <span class="text-xs text-slate-500 transition-transform {advancedUpstreamExpanded[upstreamIndex] ? 'rotate-180' : ''}">
                ▼
              </span>
            </button>

            {#if advancedUpstreamExpanded[upstreamIndex]}
              <div class="mt-2 grid gap-3 rounded-lg border border-slate-700 bg-slate-950/50 p-3 sm:grid-cols-2 lg:grid-cols-3">
                <label class="space-y-1">
                  <span class="text-[11px] font-medium text-slate-500">Connect Timeout (ms)</span>
                  <input
                    type="number"
                    min="0"
                    class="w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs tabular-nums text-slate-100 placeholder:text-slate-600 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                    disabled={isViewMode}
                    value={upstream.connect_timeout_ms ?? ''}
                    placeholder="null"
                    on:input={(e) => updateUpstream(upstreamIndex, 'connect_timeout_ms', toNullableNumber(inputValue(e)))}
                  />
                </label>

                <label class="space-y-1">
                  <span class="text-[11px] font-medium text-slate-500">Total Connect Timeout (ms)</span>
                  <input
                    type="number"
                    min="0"
                    class="w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs tabular-nums text-slate-100 placeholder:text-slate-600 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                    disabled={isViewMode}
                    value={upstream.total_connect_timeout_ms ?? ''}
                    placeholder="null"
                    on:input={(e) => updateUpstream(upstreamIndex, 'total_connect_timeout_ms', toNullableNumber(inputValue(e)))}
                  />
                </label>

                <label class="space-y-1">
                  <span class="text-[11px] font-medium text-slate-500">Read Timeout (ms)</span>
                  <input
                    type="number"
                    min="0"
                    class="w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs tabular-nums text-slate-100 placeholder:text-slate-600 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                    disabled={isViewMode}
                    value={upstream.read_timeout_ms ?? ''}
                    placeholder="null"
                    on:input={(e) => updateUpstream(upstreamIndex, 'read_timeout_ms', toNullableNumber(inputValue(e)))}
                  />
                </label>

                <label class="space-y-1">
                  <span class="text-[11px] font-medium text-slate-500">Write Timeout (ms)</span>
                  <input
                    type="number"
                    min="0"
                    class="w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs tabular-nums text-slate-100 placeholder:text-slate-600 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                    disabled={isViewMode}
                    value={upstream.write_timeout_ms ?? ''}
                    placeholder="null"
                    on:input={(e) => updateUpstream(upstreamIndex, 'write_timeout_ms', toNullableNumber(inputValue(e)))}
                  />
                </label>

                <label class="space-y-1">
                  <span class="text-[11px] font-medium text-slate-500">Idle Timeout (ms)</span>
                  <input
                    type="number"
                    min="0"
                    class="w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs tabular-nums text-slate-100 placeholder:text-slate-600 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 disabled:cursor-not-allowed disabled:bg-slate-800/50 disabled:text-slate-500"
                    disabled={isViewMode}
                    value={upstream.idle_timeout_ms ?? ''}
                    placeholder="null"
                    on:input={(e) => updateUpstream(upstreamIndex, 'idle_timeout_ms', toNullableNumber(inputValue(e)))}
                  />
                </label>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if route.upstreams.length === 0}
      <div class="px-5 py-10 text-center">
        <p class="text-sm text-slate-500">No upstreams configured.</p>
        {#if !isViewMode}
          <button
            class="mt-3 text-sm font-medium text-emerald-400 transition-colors hover:text-emerald-300"
            on:click={handleAddUpstream}
          >
            Add your first upstream →
          </button>
        {/if}
      </div>
    {/if}
  </section>
</div>