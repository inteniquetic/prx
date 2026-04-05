<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import { configStore } from '../../stores/config';
  import type { PrxConfig } from '../../types/config';

  // ---------------------------------------------------------------------------
  // Types
  // ---------------------------------------------------------------------------

  type SettingsTab = 'general' | 'tls' | 'observability' | 'toml';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export let config: PrxConfig;
  export let tomlPreview: string;
  export let validationIssues: string[];
  export let isSaving: boolean;
  export let isLoading: boolean;
  export let statusMessage: string;
  export let statusTone: 'neutral' | 'ok' | 'error';
  export let lastSynced: string;

  // ---------------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------------

  const dispatch = createEventDispatcher<{
    save: void;
    reload: void;
    exportJson: void;
    importJson: Event;
  }>();

  // ---------------------------------------------------------------------------
  // Tab State
  // ---------------------------------------------------------------------------

  let activeTab: SettingsTab = 'general';

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'tls', label: 'TLS' },
    { id: 'observability', label: 'Observability' },
    { id: 'toml', label: 'TOML Config' },
  ];

  // ---------------------------------------------------------------------------
  // Local UI State
  // ---------------------------------------------------------------------------

  let newListenAddr = '';
  let copiedToml = false;
  let importInputRef: HTMLInputElement | undefined = undefined;

  // ---------------------------------------------------------------------------
  // Computed
  // ---------------------------------------------------------------------------

  $: isValid = validationIssues.length === 0;
  $: tlsEnabled = config.server.tls !== null;
  $: tlsMissingPaths = tlsEnabled && (
    !config.server.tls?.cert_path.trim() || !config.server.tls?.key_path.trim()
  );

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;

  const parseNullableNumber = (value: string): number | null => {
    const trimmed = value.trim();
    if (!trimmed) return null;
    const num = Number(trimmed);
    return Number.isFinite(num) ? Math.max(0, Math.floor(num)) : null;
  };

  const parseRequiredNumber = (value: string): number => {
    const num = Number(value.trim());
    return Number.isFinite(num) ? Math.max(0, Math.floor(num)) : 0;
  };

  // ---------------------------------------------------------------------------
  // Server / General Actions
  // ---------------------------------------------------------------------------

  const addListenAddress = () => {
    const addr = newListenAddr.trim();
    if (!addr) return;
    if (config.server.listen.includes(addr)) {
      newListenAddr = '';
      return;
    }
    configStore.update((c) => {
      c.server.listen = [...c.server.listen, addr];
      return c;
    });
    newListenAddr = '';
  };

  const removeListenAddress = (index: number) => {
    configStore.update((c) => {
      c.server.listen = c.server.listen.filter((_, i) => i !== index);
      if (c.server.listen.length === 0) {
        c.server.listen = ['0.0.0.0:8080'];
      }
      return c;
    });
  };

  const updateServerField = (field: keyof PrxConfig['server'], value: string) => {
    configStore.update((c) => {
      (c.server as any)[field] = value;
      return c;
    });
  };

  const updateServerNullableNumber = (
    field: 'threads' | 'grace_period_seconds' | 'graceful_shutdown_timeout_seconds',
    value: string
  ) => {
    configStore.update((c) => {
      c.server[field] = parseNullableNumber(value);
      return c;
    });
  };

  const updateConfigReloadDebounce = (value: string) => {
    configStore.update((c) => {
      c.server.config_reload_debounce_ms = parseRequiredNumber(value);
      return c;
    });
  };

  // ---------------------------------------------------------------------------
  // TLS Actions
  // ---------------------------------------------------------------------------

  const toggleTls = (enabled: boolean) => {
    configStore.update((c) => {
      if (enabled) {
        c.server.tls = {
          listen: '0.0.0.0:8443',
          cert_path: '',
          key_path: '',
          enable_h2: true,
        };
      } else {
        c.server.tls = null;
      }
      return c;
    });
  };

  const updateTlsField = (field: keyof NonNullable<PrxConfig['server']['tls']>, value: string) => {
    configStore.update((c) => {
      if (c.server.tls) {
        (c.server.tls as any)[field] = value;
      }
      return c;
    });
  };

  const toggleTlsH2 = (enabled: boolean) => {
    configStore.update((c) => {
      if (c.server.tls) {
        c.server.tls.enable_h2 = enabled;
      }
      return c;
    });
  };

  // ---------------------------------------------------------------------------
  // Observability Actions
  // ---------------------------------------------------------------------------

  const logLevels = ['trace', 'debug', 'info', 'warn', 'error'] as const;

  const setLogLevel = (level: string) => {
    configStore.update((c) => {
      c.observability.log_level = level;
      return c;
    });
  };

  const toggleAccessLog = (enabled: boolean) => {
    configStore.update((c) => {
      c.observability.access_log = enabled;
      return c;
    });
  };

  const updatePrometheusListen = (value: string) => {
    configStore.update((c) => {
      c.observability.prometheus_listen = value;
      return c;
    });
  };

  // ---------------------------------------------------------------------------
  // TOML Actions
  // ---------------------------------------------------------------------------

  const copyToml = async () => {
    try {
      await navigator.clipboard.writeText(tomlPreview);
      copiedToml = true;
      setTimeout(() => { copiedToml = false; }, 2000);
    } catch {
      // Fallback: do nothing
    }
  };

  const triggerImport = () => {
    importInputRef?.click();
  };

  const handleImportFile = (event: Event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    dispatch('importJson', event);
    input.value = '';
  };

  // ---------------------------------------------------------------------------
  // Tab styling helpers
  // ---------------------------------------------------------------------------

  const tabButtonClass = (tabId: SettingsTab): string => {
    const base = 'px-4 py-2.5 text-sm font-medium transition-colors whitespace-nowrap';
    if (activeTab === tabId) {
      return `${base} border-b-2 border-cyan-400 text-cyan-300`;
    }
    return `${base} text-slate-400 hover:text-slate-200 border-b-2 border-transparent`;
  };
</script>

<AppLayout title="Settings" subtitle="Configure server, observability, and view raw config">
  <svelte:fragment slot="header-actions">
    <!-- Status indicator -->
    {#if statusMessage}
      <div
        class={
          statusTone === 'ok'
            ? 'rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-medium text-emerald-200'
            : statusTone === 'error'
              ? 'rounded-md border border-rose-400/40 bg-rose-500/10 px-3 py-1.5 text-xs font-medium text-rose-200'
              : 'rounded-md border border-slate-600 bg-slate-800 px-3 py-1.5 text-xs font-medium text-slate-300'
        }
      >
        {statusMessage}
      </div>
    {/if}

    {#if lastSynced}
      <span class="text-xs text-slate-500">Synced: {lastSynced}</span>
    {/if}

    <!-- Reload button -->
    <button
      class="rounded-md border border-slate-600 bg-slate-800 px-3 py-1.5 text-xs font-semibold text-slate-200 transition-colors hover:bg-slate-700"
      on:click={() => dispatch('reload')}
      disabled={isLoading}
    >
      {isLoading ? 'Loading...' : 'Reload'}
    </button>

    <!-- Save button -->
    <button
      class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
      on:click={() => dispatch('save')}
      disabled={isSaving}
    >
      {isSaving ? 'Saving...' : 'Save'}
    </button>
  </svelte:fragment>

  <div class="p-6">
    <!-- Tab Bar -->
    <nav class="-mb-px flex gap-0 border-b border-slate-700/80">
      {#each tabs as tab}
        <button
          class={tabButtonClass(tab.id)}
          on:click={() => (activeTab = tab.id)}
        >
          {tab.label}
          {#if tab.id === 'toml' && !isValid}
            <span class="ml-1.5 inline-flex h-4 min-w-[16px] items-center justify-center rounded-full bg-rose-500/20 px-1 text-[10px] font-bold text-rose-300">
              {validationIssues.length}
            </span>
          {/if}
        </button>
      {/each}
    </nav>

    <!-- Tab Content -->
    <div class="mt-6">
      <!-- ================================================================== -->
      <!-- GENERAL TAB                                                        -->
      <!-- ================================================================== -->
      {#if activeTab === 'general'}
        <div class="max-w-2xl space-y-6">
          <!-- Listen Addresses Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Listen Addresses</h3>
              <p class="mt-1 text-xs text-slate-500">
                Addresses the proxy listens on (e.g. 0.0.0.0:8080, [::]:8080)
              </p>
            </div>

            <!-- Tags -->
            <div class="mb-3 flex flex-wrap gap-2">
              {#each config.server.listen as addr, index}
                <span class="inline-flex items-center gap-1.5 rounded-lg border border-slate-600 bg-slate-800 px-3 py-1.5 text-sm text-slate-200">
                  <code class="text-xs text-cyan-300">{addr}</code>
                  <button
                    class="ml-0.5 rounded p-0.5 text-slate-400 transition-colors hover:bg-slate-700 hover:text-slate-200"
                    on:click={() => removeListenAddress(index)}
                    title="Remove"
                  >
                    <svg class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
                      <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
                    </svg>
                  </button>
                </span>
              {/each}
            </div>

            <!-- Add new address -->
            <div class="flex gap-2">
              <input
                type="text"
                class="flex-1 rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                placeholder="0.0.0.0:9090"
                value={newListenAddr}
                on:input={(e) => (newListenAddr = inputValue(e))}
                on:keydown={(e) => e.key === 'Enter' && addListenAddress()}
              />
              <button
                class="rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-3 py-2 text-sm font-medium text-cyan-200 transition-colors hover:bg-cyan-500/20"
                on:click={addListenAddress}
              >
                Add
              </button>
            </div>
          </section>

          <!-- Health & Readiness Paths Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Health Check Endpoints</h3>
              <p class="mt-1 text-xs text-slate-500">
                Paths for liveness and readiness probes
              </p>
            </div>

            <div class="grid gap-4 sm:grid-cols-2">
              <div>
                <label for="health-path" class="mb-1.5 block text-sm font-medium text-slate-300">
                  Health Check Path
                </label>
                <input
                  id="health-path"
                  type="text"
                  class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                  value={config.server.health_path}
                  on:input={(e) => updateServerField('health_path', inputValue(e))}
                />
                <p class="mt-1 text-xs text-slate-500">Path for liveness probe</p>
              </div>

              <div>
                <label for="ready-path" class="mb-1.5 block text-sm font-medium text-slate-300">
                  Readiness Path
                </label>
                <input
                  id="ready-path"
                  type="text"
                  class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                  value={config.server.ready_path}
                  on:input={(e) => updateServerField('ready_path', inputValue(e))}
                />
                <p class="mt-1 text-xs text-slate-500">Path for readiness probe</p>
              </div>
            </div>
          </section>

          <!-- Worker Threads -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Worker Threads</h3>
              <p class="mt-1 text-xs text-slate-500">
                Number of worker threads for the proxy server
              </p>
            </div>

            <div class="max-w-xs">
              <label for="threads" class="mb-1.5 block text-sm font-medium text-slate-300">
                Threads
              </label>
              <input
                id="threads"
                type="number"
                min="1"
                class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                placeholder="Auto (default)"
                value={config.server.threads ?? ''}
                on:input={(e) => updateServerNullableNumber('threads', inputValue(e))}
              />
              <p class="mt-1 text-xs text-slate-500">Leave empty for default (based on CPU cores)</p>
            </div>
          </section>

          <!-- Timeouts Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Graceful Shutdown & Reload</h3>
              <p class="mt-1 text-xs text-slate-500">
                Configure how the proxy handles shutdown and configuration reloads
              </p>
            </div>

            <div class="grid gap-4 sm:grid-cols-2">
              <div>
                <label for="grace-period" class="mb-1.5 block text-sm font-medium text-slate-300">
                  Grace Period
                </label>
                <div class="relative">
                  <input
                    id="grace-period"
                    type="number"
                    min="0"
                    class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 pr-12 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    placeholder="Default"
                    value={config.server.grace_period_seconds ?? ''}
                    on:input={(e) => updateServerNullableNumber('grace_period_seconds', inputValue(e))}
                  />
                  <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500">sec</span>
                </div>
                <p class="mt-1 text-xs text-slate-500">Seconds to wait during graceful reload</p>
              </div>

              <div>
                <label for="shutdown-timeout" class="mb-1.5 block text-sm font-medium text-slate-300">
                  Shutdown Timeout
                </label>
                <div class="relative">
                  <input
                    id="shutdown-timeout"
                    type="number"
                    min="0"
                    class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 pr-12 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    placeholder="Default"
                    value={config.server.graceful_shutdown_timeout_seconds ?? ''}
                    on:input={(e) => updateServerNullableNumber('graceful_shutdown_timeout_seconds', inputValue(e))}
                  />
                  <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500">sec</span>
                </div>
                <p class="mt-1 text-xs text-slate-500">Maximum time to wait for in-flight requests</p>
              </div>

              <div class="sm:col-span-2">
                <label for="reload-debounce" class="mb-1.5 block text-sm font-medium text-slate-300">
                  Config Reload Debounce
                </label>
                <div class="relative max-w-xs">
                  <input
                    id="reload-debounce"
                    type="number"
                    min="0"
                    class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 pr-16 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    value={config.server.config_reload_debounce_ms}
                    on:input={(e) => updateConfigReloadDebounce(inputValue(e))}
                  />
                  <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500">ms</span>
                </div>
                <p class="mt-1 text-xs text-slate-500">Milliseconds to debounce config file changes</p>
              </div>
            </div>
          </section>

          <!-- Save button for section -->
          <div class="flex justify-end">
            <button
              class="rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
              on:click={() => dispatch('save')}
              disabled={isSaving}
            >
              {isSaving ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </div>
      {/if}

      <!-- ================================================================== -->
      <!-- TLS TAB                                                            -->
      <!-- ================================================================== -->
      {#if activeTab === 'tls'}
        <div class="max-w-2xl space-y-6">
          <!-- Enable TLS Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="flex items-center justify-between">
              <div>
                <h3 class="text-sm font-semibold text-slate-200">Enable TLS</h3>
                <p class="mt-1 text-xs text-slate-500">
                  Enable HTTPS/TLS support for the proxy server
                </p>
              </div>
              <label class="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  class="peer sr-only"
                  checked={tlsEnabled}
                  on:change={(e) => toggleTls(e.currentTarget.checked)}
                />
                <div class="h-6 w-11 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white"></div>
              </label>
            </div>
          </section>

          {#if tlsEnabled && config.server.tls}
            <!-- Warning if paths are missing -->
            {#if tlsMissingPaths}
              <div class="rounded-xl border border-amber-400/40 bg-amber-500/10 px-4 py-3">
                <div class="flex items-start gap-3">
                  <svg class="mt-0.5 h-5 w-5 shrink-0 text-amber-400" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd" />
                  </svg>
                  <div>
                    <p class="text-sm font-medium text-amber-200">TLS is enabled but certificate paths are missing</p>
                    <p class="mt-1 text-xs text-amber-300/70">
                      Please provide both the certificate and key file paths for TLS to work properly.
                    </p>
                  </div>
                </div>
              </div>
            {/if}

            <!-- TLS Configuration -->
            <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
              <div class="mb-4">
                <h3 class="text-sm font-semibold text-slate-200">TLS Configuration</h3>
                <p class="mt-1 text-xs text-slate-500">
                  Configure TLS listen address, certificates, and protocol options
                </p>
              </div>

              <div class="space-y-4">
                <div>
                  <label for="tls-listen" class="mb-1.5 block text-sm font-medium text-slate-300">
                    Listen Address
                  </label>
                  <input
                    id="tls-listen"
                    type="text"
                    class="w-full max-w-xs rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    placeholder="0.0.0.0:8443"
                    value={config.server.tls.listen}
                    on:input={(e) => updateTlsField('listen', inputValue(e))}
                  />
                  <p class="mt-1 text-xs text-slate-500">Address for TLS connections</p>
                </div>

                <div>
                  <label for="tls-cert" class="mb-1.5 block text-sm font-medium text-slate-300">
                    Certificate Path
                  </label>
                  <input
                    id="tls-cert"
                    type="text"
                    class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    placeholder="/etc/ssl/certs/server.crt"
                    value={config.server.tls.cert_path}
                    on:input={(e) => updateTlsField('cert_path', inputValue(e))}
                  />
                  <p class="mt-1 text-xs text-slate-500">Path to the TLS certificate file (PEM format)</p>
                </div>

                <div>
                  <label for="tls-key" class="mb-1.5 block text-sm font-medium text-slate-300">
                    Private Key Path
                  </label>
                  <input
                    id="tls-key"
                    type="text"
                    class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                    placeholder="/etc/ssl/private/server.key"
                    value={config.server.tls.key_path}
                    on:input={(e) => updateTlsField('key_path', inputValue(e))}
                  />
                  <p class="mt-1 text-xs text-slate-500">Path to the TLS private key file (PEM format)</p>
                </div>

                <div class="flex items-center justify-between rounded-lg border border-slate-700/60 bg-slate-800/50 px-4 py-3">
                  <div>
                    <p class="text-sm font-medium text-slate-300">HTTP/2 Support</p>
                    <p class="mt-0.5 text-xs text-slate-500">Enable HTTP/2 protocol (h2) over TLS</p>
                  </div>
                  <label class="relative inline-flex cursor-pointer items-center">
                    <input
                      type="checkbox"
                      class="peer sr-only"
                      checked={config.server.tls.enable_h2}
                      on:change={(e) => toggleTlsH2(e.currentTarget.checked)}
                    />
                    <div class="h-6 w-11 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white"></div>
                  </label>
                </div>
              </div>
            </section>
          {/if}

          <!-- Save button for section -->
          <div class="flex justify-end">
            <button
              class="rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
              on:click={() => dispatch('save')}
              disabled={isSaving}
            >
              {isSaving ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </div>
      {/if}

      <!-- ================================================================== -->
      <!-- OBSERVABILITY TAB                                                  -->
      <!-- ================================================================== -->
      {#if activeTab === 'observability'}
        <div class="max-w-2xl space-y-6">
          <!-- Log Level Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Log Level</h3>
              <p class="mt-1 text-xs text-slate-500">
                Control the verbosity of proxy logs
              </p>
            </div>

            <!-- Segmented Control -->
            <div class="inline-flex rounded-lg border border-slate-600 bg-slate-800 p-1">
              {#each logLevels as level}
                <button
                  class="rounded-md px-3 py-1.5 text-xs font-medium transition-all"
                  class:bg-cyan-600={config.observability.log_level === level}
                  class:text-white={config.observability.log_level === level}
                  class:text-slate-400={config.observability.log_level !== level}
                  class:hover:text-slate-200={config.observability.log_level !== level}
                  class:shadow-sm={config.observability.log_level === level}
                  on:click={() => setLogLevel(level)}
                >
                  {level}
                </button>
              {/each}
            </div>

            <p class="mt-3 text-xs text-slate-500">
              Current: <code class="rounded bg-slate-800 px-1.5 py-0.5 text-cyan-300">{config.observability.log_level}</code>
              — trace is most verbose, error is least verbose
            </p>
          </section>

          <!-- Access Log Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="flex items-center justify-between">
              <div>
                <h3 class="text-sm font-semibold text-slate-200">Access Log</h3>
                <p class="mt-1 text-xs text-slate-500">
                  Log all incoming requests to stdout
                </p>
              </div>
              <label class="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  class="peer sr-only"
                  checked={config.observability.access_log}
                  on:change={(e) => toggleAccessLog(e.currentTarget.checked)}
                />
                <div class="h-6 w-11 rounded-full bg-slate-700 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-cyan-600 peer-checked:after:translate-x-full peer-checked:after:bg-white"></div>
              </label>
            </div>
          </section>

          <!-- Prometheus Section -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Prometheus Metrics</h3>
              <p class="mt-1 text-xs text-slate-500">
                Expose a /metrics endpoint for Prometheus scraping
              </p>
            </div>

            <div class="max-w-xs">
              <label for="prometheus-listen" class="mb-1.5 block text-sm font-medium text-slate-300">
                Listen Address
              </label>
              <input
                id="prometheus-listen"
                type="text"
                class="w-full rounded-lg border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
                placeholder="0.0.0.0:9090"
                value={config.observability.prometheus_listen}
                on:input={(e) => updatePrometheusListen(inputValue(e))}
              />
              <p class="mt-1 text-xs text-slate-500">
                Address for /metrics endpoint. Leave empty to disable.
              </p>
            </div>

            {#if config.observability.prometheus_listen}
              <div class="mt-3 inline-flex items-center gap-2 rounded-lg border border-emerald-400/30 bg-emerald-500/10 px-3 py-2">
                <span class="h-2 w-2 rounded-full bg-emerald-400" />
                <span class="text-xs font-medium text-emerald-200">
                  Metrics enabled at http://{config.observability.prometheus_listen}/metrics
                </span>
              </div>
            {:else}
              <div class="mt-3 inline-flex items-center gap-2 rounded-lg border border-slate-600 bg-slate-800 px-3 py-2">
                <span class="h-2 w-2 rounded-full bg-slate-500" />
                <span class="text-xs font-medium text-slate-400">Metrics disabled</span>
              </div>
            {/if}
          </section>

          <!-- Save button for section -->
          <div class="flex justify-end">
            <button
              class="rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-semibold text-emerald-200 transition-colors hover:bg-emerald-500/20"
              on:click={() => dispatch('save')}
              disabled={isSaving}
            >
              {isSaving ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </div>
      {/if}

      <!-- ================================================================== -->
      <!-- TOML CONFIG TAB                                                    -->
      <!-- ================================================================== -->
      {#if activeTab === 'toml'}
        <div class="max-w-4xl space-y-4">
          <!-- Validation Status Banner -->
          <div
            class={
              isValid
                ? 'flex items-center justify-between rounded-xl border border-emerald-400/40 bg-emerald-500/10 px-4 py-3'
                : 'flex items-center justify-between rounded-xl border border-rose-400/40 bg-rose-500/10 px-4 py-3'
            }
          >
            <div class="flex items-center gap-3">
              {#if isValid}
                <svg class="h-5 w-5 text-emerald-400" viewBox="0 0 20 20" fill="currentColor">
                  <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
                </svg>
                <span class="text-sm font-semibold text-emerald-200">VALIDATION: PASS</span>
              {:else}
                <svg class="h-5 w-5 text-rose-400" viewBox="0 0 20 20" fill="currentColor">
                  <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" />
                </svg>
                <span class="text-sm font-semibold text-rose-200">
                  VALIDATION: FAIL ({validationIssues.length} issue{validationIssues.length !== 1 ? 's' : ''})
                </span>
              {/if}
            </div>
          </div>

          <!-- First validation issue (if any) -->
          {#if !isValid && validationIssues.length > 0}
            <div class="rounded-xl border border-rose-300/30 bg-rose-500/10 px-4 py-3">
              <p class="text-sm font-medium text-rose-200">First Issue:</p>
              <p class="mt-1 text-xs text-rose-300/80">{validationIssues[0]}</p>
            </div>
          {/if}

          <!-- TOML Preview -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4 flex items-center justify-between">
              <div>
                <h3 class="text-sm font-semibold text-slate-200">Raw TOML Configuration</h3>
                <p class="mt-1 text-xs text-slate-500">
                  This is the generated TOML config that will be saved
                </p>
              </div>
              <button
                class="inline-flex items-center gap-1.5 rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20"
                on:click={copyToml}
              >
                {#if copiedToml}
                  <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                  </svg>
                  Copied
                {:else}
                  <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                    <path d="M8 3a1 1 0 011-1h2a1 1 0 110 2H9a1 1 0 01-1-1z" />
                    <path d="M6 3a2 2 0 00-2 2v11a2 2 0 002 2h8a2 2 0 002-2V5a2 2 0 00-2-2 3 3 0 01-3 3H9a3 3 0 01-3-3z" />
                  </svg>
                  Copy
                {/if}
              </button>
            </div>

            <pre class="max-h-[60vh] overflow-auto rounded-xl border border-slate-700 bg-slate-950/70 p-4 text-xs leading-6 text-slate-300 md:text-sm">{tomlPreview}</pre>
          </section>

          <!-- Import / Export Actions -->
          <section class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-6">
            <div class="mb-4">
              <h3 class="text-sm font-semibold text-slate-200">Import / Export</h3>
              <p class="mt-1 text-xs text-slate-500">
                Import or export the configuration as JSON
              </p>
            </div>

            <div class="flex gap-3">
              <button
                class="inline-flex items-center gap-2 rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-medium text-emerald-200 transition-colors hover:bg-emerald-500/20"
                on:click={() => dispatch('exportJson')}
              >
                <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                  <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clip-rule="evenodd" />
                </svg>
                Export JSON
              </button>

              <button
                class="inline-flex items-center gap-2 rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-medium text-cyan-200 transition-colors hover:bg-cyan-500/20"
                on:click={triggerImport}
              >
                <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                  <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM6.293 6.707a1 1 0 010-1.414l3-3a1 1 0 011.414 0l3 3a1 1 0 01-1.414 1.414L11 5.414V13a1 1 0 11-2 0V5.414L7.707 6.707a1 1 0 01-1.414 0z" clip-rule="evenodd" />
                </svg>
                Import JSON
              </button>

              <!-- Hidden file input for import -->
              <input
                type="file"
                accept=".json"
                class="hidden"
                bind:this={importInputRef}
                on:change={handleImportFile}
              />
            </div>
          </section>
        </div>
      {/if}
    </div>
  </div>
</AppLayout>