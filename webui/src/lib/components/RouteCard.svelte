<script lang="ts">
  import { addUpstream, configStore, removeUpstream } from '../stores/config';
  import type { LbStrategy, RouteConfig } from '../types/config';

  export let route: RouteConfig;
  export let routeIndex: number;
  export let mode: 'view' | 'edit' = 'edit';

  const lbOptions: LbStrategy[] = ['round_robin', 'random', 'hash'];
  $: isViewMode = mode === 'view';
  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;
  const numberValue = (event: Event, fallback = 0): number =>
    Number((event.currentTarget as HTMLInputElement).value || fallback);
  const checkedValue = (event: Event): boolean =>
    (event.currentTarget as HTMLInputElement).checked;
  const selectValue = (event: Event): string =>
    (event.currentTarget as HTMLSelectElement).value;

  const updateRoute = <K extends keyof RouteConfig>(key: K, value: RouteConfig[K]) => {
    configStore.update((config) => {
      const target = config.routes[routeIndex];
      if (!target) {
        return config;
      }
      target[key] = value;
      return config;
    });
  };
  const updateRouteLb = (event: Event) => {
    updateRoute('lb', selectValue(event) as LbStrategy);
  };

  const toNullableNumber = (value: string): number | null => {
    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }
    const num = Number(trimmed);
    if (!Number.isFinite(num)) {
      return null;
    }
    return Math.max(0, Math.floor(num));
  };

  const updateCircuitBreaker = (
    key: 'enabled' | 'consecutive_failures' | 'open_ms',
    value: number | boolean
  ) => {
    configStore.update((config) => {
      const routeValue = config.routes[routeIndex];
      if (!routeValue) {
        return config;
      }
      if (key === 'enabled') {
        routeValue.circuit_breaker.enabled = Boolean(value);
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
      if (!upstream) {
        return config;
      }
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
</script>

<section class="rounded-2xl bg-white/80 p-5 shadow-panel backdrop-blur">
  <div class="mb-4 flex items-center justify-between gap-4">
    <h3 class="text-lg font-semibold text-ink">Route #{routeIndex + 1} ({mode.toUpperCase()})</h3>
  </div>

  <div class="grid gap-3 md:grid-cols-2">
    <label class="text-sm text-slate-700">Name
      <input class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.name} on:input={(e) => updateRoute('name', inputValue(e))} />
    </label>
    <label class="text-sm text-slate-700">Host (optional)
      <input class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.host} on:input={(e) => updateRoute('host', inputValue(e))} />
    </label>
    <label class="text-sm text-slate-700">Path Prefix
      <input class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.path_prefix} on:input={(e) => updateRoute('path_prefix', inputValue(e))} />
    </label>
    <label class="text-sm text-slate-700">LB Strategy
      <select class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.lb} on:change={updateRouteLb}>
        {#each lbOptions as option}
          <option value={option}>{option}</option>
        {/each}
      </select>
    </label>
    <label class="text-sm text-slate-700">Max Retries
      <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.max_retries} on:input={(e) => updateRoute('max_retries', numberValue(e))} />
    </label>
    <label class="text-sm text-slate-700">Retry Backoff (ms)
      <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.retry_backoff_ms} on:input={(e) => updateRoute('retry_backoff_ms', numberValue(e))} />
    </label>
  </div>

  <label class="mt-3 flex items-center gap-2 text-sm text-slate-700">
    <input type="checkbox" disabled={isViewMode} checked={route.is_default} on:change={(e) => updateRoute('is_default', checkedValue(e))} />
    Default Route
  </label>

  <div class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-3">
    <h4 class="text-sm font-semibold uppercase tracking-wide text-tide">Circuit Breaker</h4>
    <div class="mt-2 grid gap-3 md:grid-cols-3">
      <label class="flex items-center gap-2 text-sm text-slate-700 md:mt-7">
        <input type="checkbox" disabled={isViewMode} checked={route.circuit_breaker.enabled} on:change={(e) => updateCircuitBreaker('enabled', checkedValue(e))} />
        Enabled
      </label>
      <label class="text-sm text-slate-700">Consecutive Failures
        <input type="number" min="1" class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.circuit_breaker.consecutive_failures} on:input={(e) => updateCircuitBreaker('consecutive_failures', numberValue(e, 1))} />
      </label>
      <label class="text-sm text-slate-700">Open (ms)
        <input type="number" min="1" class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={route.circuit_breaker.open_ms} on:input={(e) => updateCircuitBreaker('open_ms', numberValue(e, 1))} />
      </label>
    </div>
  </div>

  <div class="mt-5 space-y-3">
    <div class="flex items-center justify-between">
      <h4 class="text-sm font-semibold uppercase tracking-wide text-tide">Upstreams</h4>
      {#if !isViewMode}
        <button class="rounded-lg border border-aqua/40 bg-aqua/10 px-3 py-1.5 text-sm font-medium text-sky-800 hover:bg-aqua/20" on:click={() => addUpstream(routeIndex)}>
          Add Upstream
        </button>
      {/if}
    </div>

    {#each route.upstreams as upstream, upstreamIndex}
      <article class="rounded-xl border border-slate-200 bg-slate-50 p-3">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-sm font-semibold text-slate-700">#{upstreamIndex + 1}</span>
          {#if !isViewMode}
            <button class="rounded-md border border-rose-200 bg-white px-2 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50" on:click={() => removeUpstream(routeIndex, upstreamIndex)}>
              Remove
            </button>
          {/if}
        </div>

        <div class="grid gap-3 md:grid-cols-2">
          <label class="text-sm text-slate-700">Address
            <input class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.addr} on:input={(e) => updateUpstream(upstreamIndex, 'addr', inputValue(e))} />
          </label>
          <label class="text-sm text-slate-700">SNI
            <input class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.sni} on:input={(e) => updateUpstream(upstreamIndex, 'sni', inputValue(e))} />
          </label>
          <label class="text-sm text-slate-700">Weight
            <input type="number" min="1" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.weight} on:input={(e) => updateUpstream(upstreamIndex, 'weight', numberValue(e, 1))} />
          </label>
          <label class="mt-7 flex items-center gap-2 text-sm text-slate-700">
            <input type="checkbox" disabled={isViewMode} checked={upstream.tls} on:change={(e) => updateUpstream(upstreamIndex, 'tls', checkedValue(e))} />
            TLS Enabled
          </label>
          <label class="flex items-center gap-2 text-sm text-slate-700 md:mt-7">
            <input type="checkbox" disabled={isViewMode} checked={upstream.verify_cert ?? true} on:change={(e) => updateUpstream(upstreamIndex, 'verify_cert', checkedValue(e))} />
            verify_cert
          </label>
          <label class="flex items-center gap-2 text-sm text-slate-700 md:mt-7">
            <input type="checkbox" disabled={isViewMode} checked={upstream.verify_hostname ?? true} on:change={(e) => updateUpstream(upstreamIndex, 'verify_hostname', checkedValue(e))} />
            verify_hostname
          </label>
          <label class="text-sm text-slate-700">connect_timeout_ms
            <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.connect_timeout_ms ?? ''} on:input={(e) => updateUpstream(upstreamIndex, 'connect_timeout_ms', toNullableNumber(inputValue(e)))} />
          </label>
          <label class="text-sm text-slate-700">total_connect_timeout_ms
            <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.total_connect_timeout_ms ?? ''} on:input={(e) => updateUpstream(upstreamIndex, 'total_connect_timeout_ms', toNullableNumber(inputValue(e)))} />
          </label>
          <label class="text-sm text-slate-700">read_timeout_ms
            <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.read_timeout_ms ?? ''} on:input={(e) => updateUpstream(upstreamIndex, 'read_timeout_ms', toNullableNumber(inputValue(e)))} />
          </label>
          <label class="text-sm text-slate-700">write_timeout_ms
            <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.write_timeout_ms ?? ''} on:input={(e) => updateUpstream(upstreamIndex, 'write_timeout_ms', toNullableNumber(inputValue(e)))} />
          </label>
          <label class="text-sm text-slate-700">idle_timeout_ms
            <input type="number" min="0" class="mt-1 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 disabled:bg-slate-100" disabled={isViewMode} value={upstream.idle_timeout_ms ?? ''} on:input={(e) => updateUpstream(upstreamIndex, 'idle_timeout_ms', toNullableNumber(inputValue(e)))} />
          </label>
        </div>
      </article>
    {/each}
  </div>
</section>
