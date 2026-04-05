<script lang="ts">
  import { onMount } from 'svelte';
  import Sidebar from './lib/components/layout/Sidebar.svelte';
  import AppLayout from './lib/components/layout/AppLayout.svelte';
  import DashboardPage from './lib/components/pages/DashboardPage.svelte';
  import RoutesPage from './lib/components/pages/RoutesPage.svelte';
  import SettingsPage from './lib/components/pages/SettingsPage.svelte';
  import {
    loadConfigFromAdmin,
    loadRouteHealthFromAdmin,
    saveTomlToAdmin,
    type RouteHealthItem,
    type RouteHealthResponse
  } from './lib/api/admin';
  import { normalizePrxConfig } from './lib/configNormalize';
  import {
    addRoute,
    configStore,
    removeRoute,
    tomlPreview,
    validationIssues
  } from './lib/stores/config';
  import { currentPage, navigate } from './lib/stores/navigation';
  import type { PrxConfig } from './lib/types/config';

  // Health state
  let routeHealthByIndex: Record<number, RouteHealthItem> = {};
  let routeHealthResponse: RouteHealthResponse | null = null;
  let routeHealthTomlSnapshot = '';
  let isCheckingRouteHealth = false;
  let routeHealthError = '';

  // Server interaction state
  let isLoadingFromServer = false;
  let isSavingToServer = false;
  let adminStatusMessage = 'Ready';
  let adminStatusTone: 'neutral' | 'ok' | 'error' = 'neutral';
  let lastSyncedAt = '';

  // Helpers
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
    routeHealthResponse = null;
    routeHealthTomlSnapshot = '';
    routeHealthError = '';
  };

  // API actions
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
      lastSyncedAt = currentTimestamp();
      setAdminStatus('Config loaded successfully.', 'ok');
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
      payload.routes.forEach((rh) => {
        map[rh.route_index] = rh;
      });
      routeHealthByIndex = map;
      routeHealthResponse = payload;
      routeHealthTomlSnapshot = tomlSnapshot;
    } catch (error) {
      routeHealthError = toErrorMessage(error);
      setAdminStatus(`Health check failed: ${routeHealthError}`, 'error');
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
        `Save blocked: ${$validationIssues.length} validation issue(s).`,
        'error'
      );
      return;
    }

    isSavingToServer = true;
    setAdminStatus('Saving config...');

    try {
      const result = await saveTomlToAdmin($tomlPreview);
      lastSyncedAt = currentTimestamp();
      setAdminStatus(`Saved: ${result}`, 'ok');
      void refreshRouteHealth();
    } catch (error) {
      setAdminStatus(`Save failed: ${toErrorMessage(error)}`, 'error');
    } finally {
      isSavingToServer = false;
    }
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
      setAdminStatus('JSON imported locally. Click Save to apply.', 'neutral');
    } catch (error) {
      setAdminStatus(`Import failed: ${toErrorMessage(error)}`, 'error');
    } finally {
      input.value = '';
    }
  };

  // Route actions
  const addRouteAndEdit = () => {
    addRoute();
    clearRouteHealthState();
    navigate('routes');
  };

  const deleteRoute = (index: number) => {
    if (!confirm('Delete this route?')) {
      return;
    }
    removeRoute(index);
    clearRouteHealthState();
  };

  const duplicateRoute = (index: number) => {
    const source = $configStore.routes[index];
    if (!source) return;
    const clone = JSON.parse(JSON.stringify(source)) as PrxConfig['routes'][0];
    clone.name = `${clone.name}-copy`;
    configStore.update((config) => {
      config.routes.splice(index + 1, 0, clone);
      return config;
    });
    clearRouteHealthState();
  };

  // Page event handlers
  const onDashboardNavigate = (e: CustomEvent) => {
    navigate(e.detail);
  };

  const onDashboardAddRoute = () => {
    addRouteAndEdit();
  };

  const onDashboardRefreshHealth = () => {
    void refreshRouteHealth();
  };

  const onDashboardExportJson = () => {
    exportAsJson();
  };

  const onRoutesAddRoute = () => {
    addRouteAndEdit();
  };

  const onRoutesDeleteRoute = (e: CustomEvent<number>) => {
    deleteRoute(e.detail);
  };

  const onRoutesDuplicateRoute = (e: CustomEvent<number>) => {
    duplicateRoute(e.detail);
  };

  const onRoutesRefreshHealth = () => {
    void refreshRouteHealth();
  };

  const onRoutesNavigate = (e: CustomEvent) => {
    navigate(e.detail);
  };

  const onSettingsSave = () => {
    void saveToServer();
  };

  const onSettingsReload = () => {
    void reloadFromServer();
  };

  const onSettingsExportJson = () => {
    exportAsJson();
  };

  const onSettingsImportJson = (e: CustomEvent<Event>) => {
    importFromJson(e.detail);
  };

  // Reactive: clear health state when config changes
  $: if (
    routeHealthTomlSnapshot &&
    !isCheckingRouteHealth &&
    $tomlPreview !== routeHealthTomlSnapshot
  ) {
    clearRouteHealthState();
  }

  onMount(() => {
    void reloadFromServer();
  });
</script>

<div class="flex h-screen w-screen overflow-hidden text-slate-100">
  <Sidebar />

  <main class="flex-1 overflow-hidden bg-slate-950">
    {#if $currentPage === 'dashboard'}
      <DashboardPage
        config={$configStore}
        routeHealth={routeHealthResponse}
        healthLoading={isCheckingRouteHealth}
        healthError={routeHealthError}
        on:navigate={onDashboardNavigate}
        on:addRoute={onDashboardAddRoute}
        on:refreshHealth={onDashboardRefreshHealth}
        on:exportJson={onDashboardExportJson}
      />
    {:else if $currentPage === 'routes'}
      <RoutesPage
        config={$configStore}
        {routeHealthByIndex}
        healthLoading={isCheckingRouteHealth}
        healthError={routeHealthError}
        on:addRoute={onRoutesAddRoute}
        on:deleteRoute={onRoutesDeleteRoute}
        on:duplicateRoute={onRoutesDuplicateRoute}
        on:refreshHealth={onRoutesRefreshHealth}
        on:navigate={onRoutesNavigate}
      />
    {:else if $currentPage === 'settings'}
      <SettingsPage
        config={$configStore}
        tomlPreview={$tomlPreview}
        validationIssues={$validationIssues}
        isSaving={isSavingToServer}
        isLoading={isLoadingFromServer}
        statusMessage={adminStatusMessage}
        statusTone={adminStatusTone}
        lastSynced={lastSyncedAt}
        on:save={onSettingsSave}
        on:reload={onSettingsReload}
        on:exportJson={onSettingsExportJson}
        on:importJson={onSettingsImportJson}
      />
    {/if}
  </main>
</div>