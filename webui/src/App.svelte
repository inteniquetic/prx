<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import AppShell from './lib/components/layout/AppShell.svelte';
  import DashboardPage from './lib/components/pages/DashboardPage.svelte';
  import PlaceholderPage from './lib/components/pages/PlaceholderPage.svelte';
  import RoutesPage from './lib/components/pages/RoutesPage.svelte';
  import ServicesPage from './lib/components/pages/ServicesPage.svelte';
  import SettingsPage from './lib/components/pages/SettingsPage.svelte';
  import {
    loadConfigFromAdmin,
    loadRouteHealthFromAdmin,
    saveTomlToAdmin,
    type RouteHealthItem,
    type RouteHealthResponse
  } from './lib/api/admin';
  import { normalizePrxConfig } from './lib/configNormalize';
  import { configStore, tomlPreview, validationIssues } from './lib/stores/config';
  import {
    currentName,
    currentPage,
    initRouter,
    navigate,
    type NavBadge,
    type NavPage
  } from './lib/stores/navigation';
  import { startHeartbeat } from './lib/stores/connection';
  import { toast } from './lib/components/ui/sonner';
  import { initTheme } from './lib/stores/theme';
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

  // Set once the admin API has answered, so nothing judges a URL against the
  // placeholder config the store starts with.
  let configLoaded = false;

  // The TOML the proxy is running, as far as this tab knows. Anything the
  // editor produces that differs from it is an unapplied draft, which the
  // topbar says out loud so nobody closes the tab thinking it was saved.
  let appliedToml = '';

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
      appliedToml = get(tomlPreview);
      configLoaded = true;
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
      const saved = $tomlPreview;
      const result = await saveTomlToAdmin(saved);
      appliedToml = saved;
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
  //
  // Creating a route is the Routes page's job now — it talks to the admin API
  // and the change applies immediately. The shell only asks for the form, so
  // "Add route" from the dashboard or the palette lands in the same place as
  // the button on the page itself.
  let createRouteRequest = 0;
  let createServiceRequest = 0;

  const addRouteAndEdit = () => {
    navigate('routes');
    createRouteRequest += 1;
  };

  // Page event handlers
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

  const openRoute = (name: string | null) => {
    navigate(name ? { page: 'routes', name } : 'routes');
  };

  const openService = (name: string | null) => {
    navigate(name ? { page: 'services', name } : 'services');
  };


  // A deep link to something that is no longer in the config: drop back to the
  // list and say why, rather than leaving the address bar naming a route that
  // is not on screen.
  $: if (configLoaded && $currentName) {
    if (
      $currentPage === 'routes' &&
      !$configStore.routes.some((route) => route.name === $currentName)
    ) {
      toast.error(`No route named “${$currentName}”`);
      navigate('routes', { replace: true });
    } else if (
      $currentPage === 'services' &&
      !$configStore.services.some((service) => service.name === $currentName)
    ) {
      toast.error(`No service named “${$currentName}”`);
      navigate('services', { replace: true });
    }
  }

  // Shell state
  $: hasDraft = appliedToml !== '' && $tomlPreview !== appliedToml;

  $: unhealthyRoutes = routeHealthResponse
    ? routeHealthResponse.routes.filter((route) => !route.healthy).length
    : 0;

  $: downUpstreams = routeHealthResponse
    ? routeHealthResponse.routes.reduce(
        (total, route) => total + (route.total_upstreams - route.reachable_upstreams),
        0
      )
    : 0;

  // Counts only appear once a probe has actually run: a silent zero and "we
  // never checked" are not the same thing.
  $: navBadges = {
    ...(unhealthyRoutes > 0
      ? {
          routes: {
            count: unhealthyRoutes,
            label: unhealthyRoutes === 1 ? 'route unhealthy' : 'routes unhealthy',
            tone: 'destructive'
          } as NavBadge
        }
      : {}),
    ...(downUpstreams > 0
      ? {
          services: {
            count: downUpstreams,
            label: downUpstreams === 1 ? 'upstream down' : 'upstreams down',
            tone: 'warning'
          } as NavBadge
        }
      : {})
  } as Partial<Record<NavPage, NavBadge>>;

  // Reactive: clear health state when config changes
  $: if (
    routeHealthTomlSnapshot &&
    !isCheckingRouteHealth &&
    $tomlPreview !== routeHealthTomlSnapshot
  ) {
    clearRouteHealthState();
  }

  // A draft lives only in this tab: closing it throws the edit away, so the
  // browser gets a chance to ask first.
  const onBeforeUnload = (event: BeforeUnloadEvent) => {
    if (!hasDraft) return;
    event.preventDefault();
    // Safari and older Chrome still look at returnValue rather than the
    // cancelled event.
    event.returnValue = '';
  };

  onMount(() => {
    // index.html already set the class before paint; this keeps `system`
    // following the OS while the page stays open.
    const stopTheme = initTheme();
    const stopRouter = initRouter();
    void reloadFromServer();
    // Keeps the topbar's "online" honest between user actions.
    const stopHeartbeat = startHeartbeat(() => loadConfigFromAdmin());

    return () => {
      stopHeartbeat();
      stopRouter();
      stopTheme();
    };
  });
</script>

<svelte:window on:beforeunload={onBeforeUnload} />

<AppShell
  config={$configStore}
  badges={navBadges}
  {hasDraft}
  onaddRoute={addRouteAndEdit}
  onapplyDraft={() => void saveToServer()}
  onrefreshHealth={() => void refreshRouteHealth()}
>
  {#if $currentPage === 'dashboard'}
    <!-- The dashboard reads the live-stats stream itself (T306); the config is
         only there for what it cannot know from traffic, like which routes
         cache. -->
    <DashboardPage
      config={$configStore}
      onnavigate={(page) => navigate(page)}
      onselectRoute={openRoute}
      onselectService={openService}
      onaddRoute={addRouteAndEdit}
    />
  {:else if $currentPage === 'services'}
    <ServicesPage
      config={$configStore}
      selectedServiceName={$currentName}
      createRequest={createServiceRequest}
      onselect={openService}
      onchanged={() => void reloadFromServer()}
      onnavigate={(page) => navigate(page)}
    />
  {:else if $currentPage === 'routes'}
    <RoutesPage
      config={$configStore}
      selectedRouteName={$currentName}
      {routeHealthByIndex}
      createRequest={createRouteRequest}
      healthLoading={isCheckingRouteHealth}
      healthError={routeHealthError}
      onselect={openRoute}
      onchanged={() => void reloadFromServer()}
      onrefreshHealth={() => void refreshRouteHealth()}
      onnavigate={(page) => navigate(page)}
    />
  {:else if $currentPage === 'tls'}
    <PlaceholderPage
      title="TLS"
      subtitle="Certificates, SNI and ACME"
      task="T308"
      description="Certificates are configured in Settings for now. This page will show what is loaded, when each certificate expires and how ACME renewal is going."
    />
  {:else if $currentPage === 'audit'}
    <PlaceholderPage
      title="Audit"
      subtitle="Who changed what, and when"
      task="T206"
      description="The admin API does not record a change log yet. Once it does, every config apply will be listed here with its author and diff."
    />
  {:else if $currentPage === 'settings'}
    <SettingsPage
      config={$configStore}
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
</AppShell>
