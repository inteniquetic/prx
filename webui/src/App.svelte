<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import AppShell from './lib/components/layout/AppShell.svelte';
  import DashboardPage from './lib/components/pages/DashboardPage.svelte';
  import PlaceholderPage from './lib/components/pages/PlaceholderPage.svelte';
  import RoutesPage from './lib/components/pages/RoutesPage.svelte';
  import ServicesPage from './lib/components/pages/ServicesPage.svelte';
  import SettingsPage from './lib/components/pages/SettingsPage.svelte';
  import TlsPage from './lib/components/pages/TlsPage.svelte';
  import {
    loadConfigFromAdmin,
    loadRouteHealthFromAdmin,
    type RouteHealthItem,
    type RouteHealthResponse
  } from './lib/api/admin';
  import { configStore, tomlPreview } from './lib/stores/config';
  import { isDirty, loadBase, watchExternalChanges } from './lib/stores/configDraft';
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

  // Health state
  let routeHealthByIndex: Record<number, RouteHealthItem> = {};
  let routeHealthResponse: RouteHealthResponse | null = null;
  let routeHealthTomlSnapshot = '';
  let isCheckingRouteHealth = false;
  let routeHealthError = '';

  // Server interaction state
  let isLoadingFromServer = false;

  // Set once the admin API has answered, so nothing judges a URL against the
  // placeholder config the store starts with.
  let configLoaded = false;

  // Bumped to open the review from the command palette or the topbar: the
  // draft itself lives in stores/configDraft, which every page shares.
  let reviewRequest = 0;

  // Helpers
  const toErrorMessage = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

  const clearRouteHealthState = () => {
    routeHealthByIndex = {};
    routeHealthResponse = null;
    routeHealthTomlSnapshot = '';
    routeHealthError = '';
  };

  // API actions
  const reloadFromServer = async () => {
    if (isLoadingFromServer) {
      return;
    }

    isLoadingFromServer = true;
    let loadedSuccessfully = false;

    try {
      const config = await loadConfigFromAdmin();
      configStore.set(config);
      configLoaded = true;
      clearRouteHealthState();
      loadedSuccessfully = true;
    } catch (error) {
      // The connection badge already says the API is unreachable; this says
      // what the page was trying to do when it found out.
      toast.error(`Could not load the config: ${toErrorMessage(error)}`);
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
    } finally {
      isCheckingRouteHealth = false;
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

  // Shell state: the one draft every page shares (T307/T308).
  $: hasDraft = $isDirty;

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
    // The config draft belongs to the whole shell, not to the Settings page:
    // the topbar says whether one is waiting, and the palette can open it.
    void loadBase();
    const stopWatching = watchExternalChanges();
    // Keeps the topbar's "online" honest between user actions.
    const stopHeartbeat = startHeartbeat(() => loadConfigFromAdmin());

    return () => {
      stopWatching();
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
  onapplyDraft={() => {
    navigate('settings');
    reviewRequest += 1;
  }}
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
    <TlsPage onapplied={() => void reloadFromServer()} />
  {:else if $currentPage === 'audit'}
    <PlaceholderPage
      title="Audit"
      subtitle="Who changed what, and when"
      task="T206"
      description="The admin API does not record a change log yet. Once it does, every config apply will be listed here with its author and diff."
    />
  {:else if $currentPage === 'settings'}
    <SettingsPage {reviewRequest} onapplied={() => void reloadFromServer()} />
  {/if}
</AppShell>
