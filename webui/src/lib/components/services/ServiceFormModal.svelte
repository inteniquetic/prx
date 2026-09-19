<script lang="ts">
  import {
    type ServiceConfig,
    type UpstreamConfig,
    type CircuitBreakerConfig,
    type LbStrategy,
    createDefaultService,
    createDefaultUpstream,
    createDefaultCircuitBreaker
  } from '../../types/config';

  export let service: ServiceConfig | null = null;
  export let existingNames: string[] = [];

  // Form state
  let formData: ServiceConfig;
  let circuitBreakerExpanded = false;
  let errors: Record<string, string> = {};
  let touched: Record<string, boolean> = {};

  const lbOptions: LbStrategy[] = ['round_robin', 'random', 'hash'];

  const formatLbStrategy = (lb: LbStrategy): string => {
    const labels: Record<LbStrategy, string> = {
      round_robin: 'Round Robin',
      random: 'Random',
      hash: 'Hash'
    };
    return labels[lb] ?? lb;
  };

  // Initialize form data
  $: {
    if (service) {
      formData = JSON.parse(JSON.stringify(service));
    } else {
      const nextIdx = existingNames.length + 1;
      formData = createDefaultService(nextIdx);
      formData.name = '';
      formData.upstreams = [createDefaultUpstream()];
      formData.upstreams[0].addr = '';
    }
  }

  // Validation
  const validate = (): boolean => {
    errors = {};
    touched = { ...touched, name: true, upstreams: true };

    // Name validation
    if (!formData.name.trim()) {
      errors.name = 'Service name is required';
    } else {
      const otherNames = existingNames.filter(n => n !== service?.name);
      if (otherNames.includes(formData.name.trim())) {
        errors.name = 'A service with this name already exists';
      }
    }

    // Upstreams validation
    if (formData.upstreams.length === 0) {
      errors.upstreams = 'At least one upstream is required';
    } else {
      formData.upstreams.forEach((upstream, idx) => {
        if (!upstream.addr.trim()) {
          errors[`upstream_addr_${idx}`] = 'Address is required';
        }
      });
    }

    // Circuit breaker validation
    if (formData.circuit_breaker.enabled) {
      if (!formData.circuit_breaker.consecutive_failures || formData.circuit_breaker.consecutive_failures < 1) {
        errors.cb_consecutive_failures = 'Consecutive failures must be at least 1';
      }
      if (!formData.circuit_breaker.open_ms || formData.circuit_breaker.open_ms < 1) {
        errors.cb_open_ms = 'Open duration must be at least 1ms';
      }
    }

    return Object.keys(errors).length === 0;
  };

  const markTouched = (field: string) => {
    touched = { ...touched, [field]: true };
  };

  const showError = (field: string): string | undefined => {
    if (touched[field] && errors[field]) {
      return errors[field];
    }
    return undefined;
  };

  // Helpers
  const inputValue = (e: Event): string => {
    return (e.target as HTMLInputElement).value;
  };

  const numberValue = (e: Event, min: number = 0): number => {
    const val = parseInt((e.target as HTMLInputElement).value, 10);
    return isNaN(val) ? min : Math.max(min, val);
  };

  const checkedValue = (e: Event): boolean => {
    return (e.target as HTMLInputElement).checked;
  };

  const selectLbValue = (e: Event): LbStrategy => {
    return (e.target as HTMLSelectElement).value as LbStrategy;
  };

  // Updaters
  const updateField = (field: keyof ServiceConfig, value: string | number) => {
    formData = { ...formData, [field]: value };
  };

  const updateCircuitBreaker = (field: keyof CircuitBreakerConfig, value: boolean | number) => {
    formData = {
      ...formData,
      circuit_breaker: { ...formData.circuit_breaker, [field]: value }
    };
  };

  const updateUpstream = (index: number, field: keyof UpstreamConfig, value: string | number | boolean) => {
    const newUpstreams = [...formData.upstreams];
    newUpstreams[index] = { ...newUpstreams[index], [field]: value };
    formData = { ...formData, upstreams: newUpstreams };
  };

  const handleAddUpstream = () => {
    const newUpstream = createDefaultUpstream();
    newUpstream.addr = '';
    formData = {
      ...formData,
      upstreams: [...formData.upstreams, newUpstream]
    };
  };

  const handleRemoveUpstream = (index: number) => {
    if (formData.upstreams.length <= 1) return;
    const newUpstreams = formData.upstreams.filter((_, i) => i !== index);
    formData = { ...formData, upstreams: newUpstreams };
  };

  // Actions
  const handleSave = () => {
    if (validate()) {
      const saveData: ServiceConfig = {
        ...formData,
        name: formData.name.trim(),
        upstreams: formData.upstreams.map(u => ({
          ...u,
          addr: u.addr.trim(),
          sni: u.sni.trim()
        }))
      };
      dispatch('save', saveData);
    }
  };

  const handleCancel = () => {
    dispatch('cancel');
  };

  const handleKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      handleCancel();
    }
  };

  import { createEventDispatcher } from 'svelte';
  const dispatch = createEventDispatcher();
</script>

<svelte:window on:keydown={handleKeydown} />

<!-- Modal Overlay -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
  tabindex="-1"
>
  <!-- Backdrop. A button rather than a div so dismissing the dialog by
       clicking outside it is reachable from the keyboard too. -->
  <button
    type="button"
    class="absolute inset-0 bg-black/60 backdrop-blur-sm"
    aria-label="Close dialog"
    on:click={handleCancel}
  ></button>

  <!-- Modal Content -->
  <div class="relative z-10 flex max-h-[90vh] w-full max-w-2xl flex-col rounded-xl border border-border/80 bg-background shadow-2xl">
    <!-- Header -->
    <div class="flex items-center justify-between border-b border-border/80 px-6 py-4">
      <div>
        <h2 class="text-lg font-bold text-foreground">
          {service ? 'Edit Service' : 'Create New Service'}
        </h2>
        <p class="mt-0.5 text-sm text-muted-foreground">
          {service ? 'Update the service configuration' : 'Configure a new backend service target'}
        </p>
      </div>
      <button
        class="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        aria-label="Close"
        on:click={handleCancel}
      >
        <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Scrollable Form Content -->
    <div class="flex-1 overflow-y-auto p-6">
      <div class="space-y-5">
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
              <span class="text-sm font-medium text-foreground/80">
                Name <span class="text-destructive">*</span>
              </span>
              <input
                type="text"
                class="w-full rounded-lg border px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:outline-none focus:ring-1 {showError('name') ? 'border-destructive focus:border-destructive focus:ring-destructive/30' : 'border-border bg-background/70 focus:border-primary focus:ring-primary/30'}"
                value={formData.name}
                placeholder="e.g., api-backend"
                on:input={(e) => { updateField('name', inputValue(e)); markTouched('name'); }}
              />
              {#if showError('name')}
                <p class="text-xs text-destructive">{errors.name}</p>
              {/if}
            </label>

            <!-- LB Strategy -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-foreground/80">Load Balancing Strategy</span>
              <select
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
                value={formData.lb}
                on:change={(e) => updateField('lb', selectLbValue(e))}
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
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
                value={formData.max_retries}
                on:input={(e) => updateField('max_retries', numberValue(e))}
              />
            </label>

            <!-- Retry Backoff -->
            <label class="space-y-1.5">
              <span class="text-sm font-medium text-foreground/80">
                Retry Backoff <span class="text-muted-foreground">(ms)</span>
              </span>
              <input
                type="number"
                min="0"
                class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
                value={formData.retry_backoff_ms}
                on:input={(e) => updateField('retry_backoff_ms', numberValue(e))}
              />
            </label>
          </div>
        </section>

        <!-- ============================================================= -->
        <!-- Section 2: Circuit Breaker -->
        <!-- ============================================================= -->
        <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
          <button
            class="flex w-full items-center justify-between px-5 py-4 text-left transition-colors hover:bg-muted/50"
            on:click={() => circuitBreakerExpanded = !circuitBreakerExpanded}
          >
            <div class="flex items-center gap-3">
              <div class="h-8 w-1 rounded-full bg-warning" ></div>
              <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
                Circuit Breaker
              </h3>
              {#if formData.circuit_breaker.enabled}
                <span class="rounded-full border border-warning/40 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold text-warning">
                  ON
                </span>
              {:else}
                <span class="rounded-full border border-border bg-muted px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                  OFF
                </span>
              {/if}
            </div>
            <span class="text-muted-foreground transition-transform {circuitBreakerExpanded ? 'rotate-180' : ''}">
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
                      checked={formData.circuit_breaker.enabled}
                      on:change={(e) => updateCircuitBreaker('enabled', checkedValue(e))}
                    />
                    <div class="h-6 w-11 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-warning peer-checked:after:translate-x-full peer-checked:after:bg-white"></div>
                  </label>
                </label>
              </div>

              {#if formData.circuit_breaker.enabled}
                <div class="grid gap-5 md:grid-cols-2">
                  <!-- Consecutive Failures -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-foreground/80">
                      Consecutive Failures <span class="text-destructive">*</span>
                    </span>
                    <p class="text-xs text-muted-foreground">Number of failures before tripping the breaker</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:outline-none focus:ring-1 {showError('cb_consecutive_failures') ? 'border-destructive focus:border-destructive focus:ring-destructive/30' : 'border-border bg-background/70 focus:border-primary focus:ring-primary/30'}"
                      value={formData.circuit_breaker.consecutive_failures}
                      on:input={(e) => { updateCircuitBreaker('consecutive_failures', numberValue(e, 1)); markTouched('cb_consecutive_failures'); }}
                    />
                    {#if showError('cb_consecutive_failures')}
                      <p class="text-xs text-destructive">{errors.cb_consecutive_failures}</p>
                    {/if}
                  </label>

                  <!-- Open Duration -->
                  <label class="space-y-1.5">
                    <span class="text-sm font-medium text-foreground/80">
                      Open Duration <span class="text-muted-foreground">(ms)</span> <span class="text-destructive">*</span>
                    </span>
                    <p class="text-xs text-muted-foreground">How long the breaker stays open before attempting recovery</p>
                    <input
                      type="number"
                      min="1"
                      class="w-full rounded-lg border px-3 py-2.5 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:outline-none focus:ring-1 {showError('cb_open_ms') ? 'border-destructive focus:border-destructive focus:ring-destructive/30' : 'border-border bg-background/70 focus:border-primary focus:ring-primary/30'}"
                      value={formData.circuit_breaker.open_ms}
                      on:input={(e) => { updateCircuitBreaker('open_ms', numberValue(e, 1)); markTouched('cb_open_ms'); }}
                    />
                    {#if showError('cb_open_ms')}
                      <p class="text-xs text-destructive">{errors.cb_open_ms}</p>
                    {/if}
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
                {formData.upstreams.length}
              </span>
            </div>
            <button
              class="flex items-center gap-1.5 rounded-lg border border-success/40 bg-success/10 px-3 py-1.5 text-xs font-medium text-success transition-colors hover:bg-success/20"
              on:click={handleAddUpstream}
            >
              <span class="text-sm">+</span>
              Add Upstream
            </button>
          </div>

          {#if showError('upstreams')}
            <div class="border-b border-border/60 px-5 py-2">
              <p class="text-xs text-destructive">{errors.upstreams}</p>
            </div>
          {/if}

          <div class="divide-y divide-border/60">
            {#each formData.upstreams as upstream, upstreamIndex}
              <div class="p-5">
                <!-- Upstream Header -->
                <div class="mb-4 flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <span class="flex h-6 w-6 items-center justify-center rounded-md bg-muted text-xs font-bold text-muted-foreground">
                      {upstreamIndex + 1}
                    </span>
                    <span class="text-sm font-medium text-foreground/80">
                      {upstream.addr || 'Unconfigured'}
                    </span>
                    {#if upstream.tls}
                      <span class="rounded border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                        TLS
                      </span>
                    {/if}
                  </div>
                  {#if formData.upstreams.length > 1}
                    <button
                      class="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs font-medium text-destructive transition-colors hover:bg-destructive/20"
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
                    <span class="text-xs font-medium text-muted-foreground">
                      Address <span class="text-destructive">*</span>
                    </span>
                    <input
                      type="text"
                      class="w-full rounded-lg border px-3 py-2 text-sm font-mono text-foreground placeholder:text-muted-foreground transition-colors focus:outline-none focus:ring-1 {showError(`upstream_addr_${upstreamIndex}`) ? 'border-destructive focus:border-destructive focus:ring-destructive/30' : 'border-border bg-background/70 focus:border-primary focus:ring-primary/30'}"
                      value={upstream.addr}
                      placeholder="host:port"
                      on:input={(e) => { updateUpstream(upstreamIndex, 'addr', inputValue(e)); markTouched('upstreams'); }}
                    />
                    {#if showError(`upstream_addr_${upstreamIndex}`)}
                      <p class="text-xs text-destructive">{errors[`upstream_addr_${upstreamIndex}`]}</p>
                    {/if}
                  </label>

                  <!-- Weight -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-muted-foreground">
                      Weight <span class="text-muted-foreground/70">(1-256)</span>
                    </span>
                    <input
                      type="number"
                      min="1"
                      max="256"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2 text-sm tabular-nums text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
                      value={upstream.weight}
                      on:input={(e) => updateUpstream(upstreamIndex, 'weight', numberValue(e, 1))}
                    />
                  </label>

                  <!-- SNI -->
                  <label class="space-y-1.5">
                    <span class="text-xs font-medium text-muted-foreground">
                      SNI <span class="text-muted-foreground/70">(TLS)</span>
                    </span>
                    <input
                      type="text"
                      class="w-full rounded-lg border border-border bg-background/70 px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
                      value={upstream.sni}
                      placeholder="hostname"
                      on:input={(e) => updateUpstream(upstreamIndex, 'sni', inputValue(e))}
                    />
                  </label>

                  <!-- TLS Toggle -->
                  <div class="flex items-end pb-0.5 md:col-span-2">
                    <label class="flex w-full items-center justify-between gap-3 rounded-lg border border-border bg-background/70 px-3 py-2">
                      <span class="text-xs font-medium text-muted-foreground">TLS Enabled</span>
                      <label class="relative inline-flex cursor-pointer items-center">
                        <input
                          type="checkbox"
                          class="peer sr-only"
                          checked={upstream.tls}
                          on:change={(e) => updateUpstream(upstreamIndex, 'tls', checkedValue(e))}
                        />
                        <div class="h-5 w-9 rounded-full bg-muted after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-muted-foreground after:transition-all peer-checked:bg-primary peer-checked:after:translate-x-full peer-checked:after:bg-white"></div>
                      </label>
                    </label>
                  </div>
                </div>
              </div>
            {/each}
          </div>

          {#if formData.upstreams.length === 0}
            <div class="px-5 py-10 text-center">
              <p class="text-sm text-muted-foreground">No upstreams configured</p>
              <button
                class="mt-2 text-sm font-medium text-success hover:text-success"
                on:click={handleAddUpstream}
              >
                + Add an upstream
              </button>
            </div>
          {/if}
        </section>
      </div>
    </div>

    <!-- Footer -->
    <div class="flex items-center justify-end gap-3 border-t border-border/80 px-6 py-4">
      <button
        class="rounded-lg border border-border bg-muted px-4 py-2.5 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted"
        on:click={handleCancel}
      >
        Cancel
      </button>
      <button
        class="rounded-lg bg-primary px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-primary"
        on:click={handleSave}
      >
        {service ? 'Save Changes' : 'Create Service'}
      </button>
    </div>
  </div>
</div>