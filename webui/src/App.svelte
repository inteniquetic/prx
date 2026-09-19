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
  import SetupWizard from './lib/components/onboarding/SetupWizard.svelte';
  import ErrorScreen from './lib/components/layout/ErrorScreen.svelte';
  import {
    loadConfigFromAdmin,
    loadRouteHealthFromAdmin,
    type RouteHealthItem,
    type RouteHealthResponse
  } from './lib/api/admin';
  import { configStore, tomlPreview } from './lib/stores/config';
  import { draftConfig, isDirty, loadBase, watchExternalChanges } from './lib/stores/configDraft';
  import { isUnconfigured } from './lib/configTemplates';
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
  import { initLocale, t } from './lib/i18n';

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

  // A proxy with no services answers every request with a 404, so the wizard
  // offers itself once. Saying no is remembered: it is an offer, not a gate.
  const WIZARD_DISMISSED_KEY = 'prx-wizard-dismissed';
  let wizardOpen = false;
  let wizardOffered = false;

  const wizardDismissed = (): boolean => {
    try {
      return localStorage.getItem(WIZARD_DISMISSED_KEY) === '1';
    } catch {
      return false;
    }
  };

  const rememberWizardDismissed = () => {
    try {
      localStorage.setItem(WIZARD_DISMISSED_KEY, '1');
    } catch {
      // Not remembering it only means the offer comes back next time.
    }
  };

  const openWizard = () => {
    wizardOpen = true;
  };

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
      toast.error($t('app.loadFailed', { error: toErrorMessage(error) }));
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
      toast.error($t('app.noRoute', { name: $currentName }));
      navigate('routes', { replace: true });
    } else if (
      $currentPage === 'services' &&
      !$configStore.services.some((service) => service.name === $currentName)
    ) {
      toast.error($t('app.noService', { name: $currentName }));
      navigate('services', { replace: true });
    }
  }

  // Shell state: the one draft every page shares (T307/T308).
  $: hasDraft = $isDirty;

  // Offered once the draft has actually loaded, so an empty placeholder config
  // on the first frame does not trigger it.
  $: if (!wizardOffered && $draftConfig && isUnconfigured($draftConfig) && !wizardDismissed()) {
    wizardOffered = true;
    wizardOpen = true;
  }

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
            labelKey:
              unhealthyRoutes === 1 ? 'nav.badge.routeUnhealthy' : 'nav.badge.routesUnhealthy',
            tone: 'destructive'
          } as NavBadge
        }
      : {}),
    ...(downUpstreams > 0
      ? {
          services: {
            count: downUpstreams,
            labelKey:
              downUpstreams === 1 ? 'nav.badge.upstreamDown' : 'nav.badge.upstreamsDown',
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
    // `<html lang>` follows the chosen language, which is what a screen reader
    // reads it in.
    initLocale();
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
  <!-- A component that throws should cost one page, not the whole UI: the
       boundary keeps the shell — and the way back out — on screen (T309). -->
  <svelte:boundary>
    {#snippet failed(error, reset)}
      <ErrorScreen {error} {reset} />
    {/snippet}

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
      onsetup={openWizard}
    />
    {:else if $currentPage === 'services'}
    <ServicesPage
      config={$configStore}
      selectedServiceName={$currentName}
      createRequest={createServiceRequest}
      loading={!configLoaded}
      onselect={openService}
      onchanged={() => void reloadFromServer()}
      onnavigate={(page) => navigate(page)}
      onsetup={openWizard}
    />
    {:else if $currentPage === 'routes'}
    <RoutesPage
      config={$configStore}
      selectedRouteName={$currentName}
      {routeHealthByIndex}
      createRequest={createRouteRequest}
      loading={!configLoaded}
      healthLoading={isCheckingRouteHealth}
      healthError={routeHealthError}
      onselect={openRoute}
      onchanged={() => void reloadFromServer()}
      onrefreshHealth={() => void refreshRouteHealth()}
      onnavigate={(page) => navigate(page)}
      onsetup={openWizard}
    />
    {:else if $currentPage === 'tls'}
    <TlsPage onapplied={() => void reloadFromServer()} />
    {:else if $currentPage === 'audit'}
    <PlaceholderPage
      title={$t('audit.title')}
      subtitle={$t('audit.subtitle')}
      task="T206"
      description={$t('audit.body')}
    />
    {:else if $currentPage === 'settings'}
    <SettingsPage {reviewRequest} onapplied={() => void reloadFromServer()} />
    {/if}
  </svelte:boundary>
</AppShell>

<SetupWizard
  bind:open={wizardOpen}
  onfinished={() => {
    navigate('settings');
    reviewRequest += 1;
  }}
  ondismissed={rememberWizardDismissed}
/>
