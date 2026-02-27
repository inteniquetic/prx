<script lang="ts">
  import { onMount } from 'svelte';

  import AppToolbar from './lib/components/dashboard/AppToolbar.svelte';
  import ObservabilityCard from './lib/components/dashboard/ObservabilityCard.svelte';
  import RouteDetailCard from './lib/components/dashboard/RouteDetailCard.svelte';
  import RouteTableCard from './lib/components/dashboard/RouteTableCard.svelte';
  import ServerCard from './lib/components/dashboard/ServerCard.svelte';
  import TomlPreviewCard from './lib/components/dashboard/TomlPreviewCard.svelte';
  import {
    loadConfigFromAdmin,
    loadRouteHealthFromAdmin,
    saveTomlToAdmin,
    type RouteHealthItem
  } from './lib/api/admin';
  import { normalizePrxConfig } from './lib/configNormalize';
  import {
    addRoute,
    configStore,
    removeRoute,
    tomlPreview,
    validationIssues
  } from './lib/stores/config';
  import type { PrxConfig } from './lib/types/config';

  let copied = false;
  let routePanelMode: 'view' | 'edit' = 'view';
  let selectedRouteIndex: number | null = null;
  let routeQuery = '';
  let routeHealthByIndex: Record<number, RouteHealthItem> = {};
  let routeHealthTomlSnapshot = '';
  let isCheckingRouteHealth = false;
  let routeHealthError = '';

  let isLoadingFromServer = false;
  let isSavingToServer = false;
  let adminStatusMessage = 'Ready';
  let adminStatusTone: 'neutral' | 'ok' | 'error' = 'neutral';
  let lastSyncedAt = '';

  const currentTimestamp = (): string =>
    new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    }).format(new Date());

  const toErrorMessage = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

  const setAdminStatus = (message: string, tone: 'neutral' | 'ok' | 'error' = 'neutral') => {
    adminStatusMessage = message;
    adminStatusTone = tone;
  };

  const clearRouteHealthState = () => {
    routeHealthByIndex = {};
    routeHealthTomlSnapshot = '';
    routeHealthError = '';
  };

  const reloadFromServer = async () => {
    if (isLoadingFromServer || isSavingToServer) {
      return;
    }

    isLoadingFromServer = true;
    let loadedSuccessfully = false;
    setAdminStatus('Loading config from admin API...');

    try {
      const config = await loadConfigFromAdmin();
      configStore.set(config);
      clearRouteHealthState();
      if (selectedRouteIndex !== null && selectedRouteIndex >= config.routes.length) {
        selectedRouteIndex = null;
      }
      lastSyncedAt = currentTimestamp();
      setAdminStatus('Loaded config from admin API.', 'ok');
      loadedSuccessfully = true;
    } catch (error) {
      setAdminStatus(`Load failed: ${toErrorMessage(error)}`, 'error');
    } finally {
      isLoadingFromServer = false;
      if (loadedSuccessfully) {
        void refreshRouteHealth();
      }
    }
  };

  const refreshRouteHealth = async () => {
    if (isCheckingRouteHealth || isLoadingFromServer) {
      return;
    }

    const tomlSnapshot = $tomlPreview;
    isCheckingRouteHealth = true;
    routeHealthError = '';
    try {
      const payload = await loadRouteHealthFromAdmin(1200, tomlSnapshot);
      const map: Record<number, RouteHealthItem> = {};
      payload.routes.forEach((routeHealth) => {
        map[routeHealth.route_index] = routeHealth;
      });
      routeHealthByIndex = map;
      routeHealthTomlSnapshot = tomlSnapshot;
    } catch (error) {
      routeHealthError = toErrorMessage(error);
      setAdminStatus(`Route health check failed: ${routeHealthError}`, 'error');
    } finally {
      isCheckingRouteHealth = false;
    }
  };

  const saveToServer = async () => {
    if (isSavingToServer || isLoadingFromServer) {
      return;
    }

    if ($validationIssues.length > 0) {
      setAdminStatus(
        `Save blocked: validation failed (${$validationIssues.length} issue(s)).`,
        'error'
      );
      return;
    }

    isSavingToServer = true;
    setAdminStatus('Saving config to admin API...');

    try {
      const result = await saveTomlToAdmin($tomlPreview);
      lastSyncedAt = currentTimestamp();
      setAdminStatus(`Save successful: ${result}`, 'ok');
      void refreshRouteHealth();
    } catch (error) {
      setAdminStatus(`Save failed: ${toErrorMessage(error)}`, 'error');
    } finally {
      isSavingToServer = false;
    }
  };

  const updateServer = (server: PrxConfig['server']) => {
    configStore.update((config) => {
      config.server = server;
      return config;
    });
  };

  const updateObservability = <K extends keyof PrxConfig['observability']>(
    key: K,
    value: PrxConfig['observability'][K]
  ) => {
    configStore.update((config) => {
      config.observability[key] = value;
      return config;
    });
  };

  const exportAsJson = () => {
    const payload = JSON.stringify($configStore, null, 2);
    const blob = new Blob([payload], { type: 'application/json' });
    const href = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = href;
    a.download = 'prx-config.json';
    a.click();
    URL.revokeObjectURL(href);
  };

  const importFromJson = async (event: Event) => {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.files || input.files.length === 0) {
      return;
    }

    try {
      const file = input.files[0];
      const content = await file.text();
      const parsed = JSON.parse(content) as Partial<PrxConfig>;
      configStore.set(normalizePrxConfig(parsed));
      clearRouteHealthState();
      setAdminStatus('Imported JSON locally. Click Save to Server to apply.', 'neutral');
    } catch (error) {
      setAdminStatus(`Import failed: ${toErrorMessage(error)}`, 'error');
    } finally {
      input.value = '';
    }
  };

  const onImportJson = (event: CustomEvent<Event>) => {
    importFromJson(event.detail);
  };

  const onServerSave = (event: CustomEvent<PrxConfig['server']>) => {
    updateServer(event.detail);
  };

  const onObservabilityChange = (
    event: CustomEvent<{
      key: keyof PrxConfig['observability'];
      value: string | boolean;
    }>
  ) => {
    const { key, value } = event.detail;
    if (key === 'access_log') {
      updateObservability('access_log', Boolean(value));
      return;
    }
    if (key === 'log_level') {
      updateObservability('log_level', String(value));
      return;
    }
    updateObservability('prometheus_listen', String(value));
  };

  const copyToml = async () => {
    await navigator.clipboard.writeText($tomlPreview);
    copied = true;
    setTimeout(() => {
      copied = false;
    }, 1800);
  };

  const openRoutePanel = (index: number, mode: 'view' | 'edit') => {
    selectedRouteIndex = index;
    routePanelMode = mode;
  };

  const addRouteAndEdit = () => {
    addRoute();
    clearRouteHealthState();
    selectedRouteIndex = $configStore.routes.length - 1;
    routePanelMode = 'edit';
  };

  const deleteRoute = (index: number) => {
    if (!confirm('Delete this route?')) {
      return;
    }
    removeRoute(index);
    clearRouteHealthState();
    if ($configStore.routes.length === 0) {
      selectedRouteIndex = null;
      return;
    }
    if (selectedRouteIndex === index) {
      selectedRouteIndex = null;
      return;
    }
    if (selectedRouteIndex !== null && selectedRouteIndex > index) {
      selectedRouteIndex -= 1;
    }
  };

  const closeRoutePanel = () => {
    selectedRouteIndex = null;
  };

  onMount(() => {
    void reloadFromServer();
  });

  $: selectedRoute =
    selectedRouteIndex === null ? null : $configStore.routes[selectedRouteIndex] ?? null;
  $: selectedRouteIndexValue = selectedRouteIndex ?? 0;
  $: if (
    routeHealthTomlSnapshot &&
    !isCheckingRouteHealth &&
    $tomlPreview !== routeHealthTomlSnapshot
  ) {
    clearRouteHealthState();
  }
  $: filteredRoutes = $configStore.routes
    .map((route, routeIndex) => ({ route, routeIndex }))
    .filter(({ route }) => {
      const query = routeQuery.trim().toLowerCase();
      if (!query) {
        return true;
      }
      return (
        route.name.toLowerCase().includes(query) ||
        route.host.toLowerCase().includes(query) ||
        route.path_prefix.toLowerCase().includes(query) ||
        route.lb.toLowerCase().includes(query)
      );
    });
</script>

<main class="min-h-screen w-full px-3 py-3 text-ink md:px-5">
  <AppToolbar
    saveDisabled={isSavingToServer || isLoadingFromServer || $validationIssues.length > 0}
    reloadDisabled={isSavingToServer || isLoadingFromServer}
    saving={isSavingToServer}
    loading={isLoadingFromServer}
    statusMessage={adminStatusMessage}
    statusTone={adminStatusTone}
    lastSynced={lastSyncedAt}
    on:saveToServer={saveToServer}
    on:reloadFromServer={reloadFromServer}
    on:exportJson={exportAsJson}
    on:importJson={onImportJson}
  />

  <div class="mt-4 space-y-4">
    <div class="grid gap-4 lg:grid-cols-2">
      <ServerCard
        server={$configStore.server}
        on:save={onServerSave}
      />
      <ObservabilityCard
        observability={$configStore.observability}
        on:change={onObservabilityChange}
      />
    </div>

    <div class="grid gap-4 xl:grid-cols-2">
      <RouteTableCard
        routeQuery={routeQuery}
        rows={filteredRoutes}
        {selectedRouteIndex}
        {routeHealthByIndex}
        healthLoading={isCheckingRouteHealth}
        healthError={routeHealthError}
        on:addRoute={addRouteAndEdit}
        on:refreshHealth={() => void refreshRouteHealth()}
        on:search={(e) => (routeQuery = e.detail)}
        on:view={(e) => openRoutePanel(e.detail, 'view')}
        on:edit={(e) => openRoutePanel(e.detail, 'edit')}
        on:delete={(e) => deleteRoute(e.detail)}
      />
      <RouteDetailCard
        route={selectedRoute}
        routeIndex={selectedRouteIndexValue}
        mode={routePanelMode}
        on:close={closeRoutePanel}
      />
    </div>

    <TomlPreviewCard toml={$tomlPreview} issues={$validationIssues} {copied} on:copy={copyToml} />
  </div>
</main>
