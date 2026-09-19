<script lang="ts">
  import ActivityIcon from '@lucide/svelte/icons/activity';
  import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import SearchIcon from '@lucide/svelte/icons/search';
  import ServerIcon from '@lucide/svelte/icons/server';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { EmptyState } from '$lib/components/ui/empty-state';
  import { Input } from '$lib/components/ui/input';
  import { StatusDot, type UpstreamStatus as DotStatus } from '$lib/components/ui/status-dot';
  import { toast } from '$lib/components/ui/sonner';

  import ServiceSheet from '../services/ServiceSheet.svelte';
  import TemplatePicker from '../onboarding/TemplatePicker.svelte';
  import { SkeletonTable } from '$lib/components/ui/skeleton';

  import {
    createService,
    deleteService,
    loadServiceStatus,
    updateService,
    type ServiceStatus,
    type UpstreamStatus
  } from '$lib/api/admin';
  import { formatLatency } from '$lib/format';
  import { routesUsing } from '$lib/serviceValidation';
  import { createDefaultService, type PrxConfig, type ServiceConfig } from '$lib/types/config';
  import { cn } from '$lib/utils';
  import { plural, t } from '$lib/i18n';

  let {
    config,
    /** The service the URL is pointing at — `/services/:name`. */
    selectedServiceName = null,
    createRequest = 0,
    /** True until the admin API has answered for the first time. */
    loading = false,
    onselect,
    onchanged,
    onnavigate,
    /** Opens the setup wizard, which lives in the shell. */
    onsetup
  }: {
    config: PrxConfig;
    selectedServiceName?: string | null;
    createRequest?: number;
    loading?: boolean;
    onselect?: (name: string | null) => void;
    onchanged?: () => void;
    onnavigate?: (page: 'routes' | 'settings') => void;
    onsetup?: () => void;
  } = $props();

  let query = $state('');
  let saving = $state(false);
  let serverError = $state<string | null>(null);
  let sheetOpen = $state(false);
  let sheetMode = $state<'create' | 'edit'>('edit');
  let sheetService = $state<ServiceConfig | null>(null);
  let confirmOpen = $state(false);
  let confirmTarget = $state<string | null>(null);

  let status = $state<ServiceStatus[]>([]);
  let statusError = $state('');
  let statusAt = $state<number | null>(null);

  const services = $derived(config.services ?? []);
  const routes = $derived(config.routes ?? []);

  const statusByService = $derived(
    new Map(status.map((entry) => [entry.name, entry]))
  );

  /** Live upstream state for one service, keyed by address. */
  const upstreamStatus = (name: string): Record<string, UpstreamStatus> => {
    const entry = statusByService.get(name);
    if (!entry) return {};
    return Object.fromEntries(entry.upstreams.map((upstream) => [upstream.addr, upstream]));
  };

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return services;
    return services.filter(
      (service) =>
        service.name.toLowerCase().includes(needle) ||
        service.upstreams.some((upstream) => upstream.addr.toLowerCase().includes(needle))
    );
  });

  const healthOf = (service: ServiceConfig): { healthy: number; total: number } => {
    const live = statusByService.get(service.name);
    const total = service.upstreams.filter((upstream) => upstream.enabled).length;
    if (!live) return { healthy: 0, total };
    const healthy = live.upstreams.filter(
      (upstream) => upstream.enabled && upstream.available && upstream.probe_healthy
    ).length;
    return { healthy, total };
  };

  const serviceDot = (service: ServiceConfig): DotStatus => {
    const live = statusByService.get(service.name);
    if (!live) return 'unknown';
    const { healthy, total } = healthOf(service);
    if (total === 0) return 'disabled';
    if (healthy === 0) return 'down';
    return healthy < total ? 'degraded' : 'healthy';
  };

  const serviceReason = (service: ServiceConfig): string => {
    const { healthy, total } = healthOf(service);
    if (total === 0) return $t('services.reason.allDrained');
    if (!statusByService.has(service.name)) {
      return $t('services.reason.unknown');
    }
    return $t('services.reason.ready', { healthy, total });
  };

  // --- live status ---------------------------------------------------------
  // The health state has to move without anyone reloading the page, and there
  // is no event stream until T207, so the page asks. It stops asking while the
  // tab is hidden: a background tab does not need to keep polling a proxy.
  const POLL_MS = 3000;

  async function refreshStatus() {
    try {
      const payload = await loadServiceStatus();
      status = payload.services;
      statusAt = payload.checked_at_epoch_ms;
      statusError = '';
    } catch (err) {
      statusError = err instanceof Error ? err.message : String(err);
    }
  }

  $effect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let stopped = false;

    const tick = async () => {
      if (stopped) return;
      if (document.visibilityState === 'visible') await refreshStatus();
      if (!stopped) timer = setTimeout(tick, POLL_MS);
    };

    void tick();
    const onVisible = () => {
      if (document.visibilityState === 'visible') void refreshStatus();
    };
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      stopped = true;
      clearTimeout(timer);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });

  // --- the sheet -----------------------------------------------------------

  function openCreate() {
    let suffix = services.length + 1;
    let candidate = `service-${suffix}`;
    while (services.some((service) => service.name === candidate)) {
      suffix += 1;
      candidate = `service-${suffix}`;
    }
    const base = createDefaultService(suffix);
    base.name = candidate;
    sheetService = base;
    sheetMode = 'create';
    serverError = null;
    sheetOpen = true;
  }

  // The URL owns which service is open (T303).
  $effect(() => {
    const name = selectedServiceName;
    if (!name) {
      if (sheetMode === 'edit' && sheetOpen) sheetOpen = false;
      return;
    }
    if (services.length === 0) return;
    const service = services.find((entry) => entry.name === name);
    if (!service) return;
    sheetService = service;
    sheetMode = 'edit';
    sheetOpen = true;
  });

  let lastCreateRequest = $state(0);
  $effect(() => {
    if (createRequest !== lastCreateRequest) {
      lastCreateRequest = createRequest;
      openCreate();
    }
  });

  function closeSheet() {
    sheetOpen = false;
    serverError = null;
    if (selectedServiceName) onselect?.(null);
  }

  async function save(service: ServiceConfig) {
    saving = true;
    serverError = null;
    try {
      if (sheetMode === 'create') {
        await createService(service);
        toast.success($t('services.toast.created', { name: service.name }));
      } else {
        await updateService(sheetService?.name ?? service.name, service);
        toast.success($t('services.toast.saved', { name: service.name }));
      }
      sheetOpen = false;
      if (selectedServiceName) onselect?.(null);
      onchanged?.();
      void refreshStatus();
    } catch (err) {
      serverError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function askDelete(name: string) {
    const blocking = routesUsing(name, routes);
    if (blocking.length > 0) {
      toast.error(
        `“${name}” still has ${blocking.length} route${blocking.length === 1 ? '' : 's'}: ${blocking
          .map((route) => route.name)
          .join(', ')}`
      );
      return;
    }
    confirmTarget = name;
    confirmOpen = true;
  }

  async function confirmDelete() {
    if (!confirmTarget || saving) return;
    saving = true;
    try {
      await deleteService(confirmTarget);
      toast.success($t('services.toast.deleted', { name: confirmTarget }));
      confirmOpen = false;
      sheetOpen = false;
      if (selectedServiceName === confirmTarget) onselect?.(null);
      onchanged?.();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      saving = false;
      confirmTarget = null;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">{$t('services.title')}</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {$plural('services.count', services.length)}
        {#if statusAt}
          ·
          {$t('services.liveState', {
            state: statusError ? $t('services.liveState.stale') : $t('services.liveState.updating')
          })}
        {/if}
        {#if statusError}
          · <span class="text-warning-emphasis">{statusError}</span>
        {/if}
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => void refreshStatus()}>
        <ActivityIcon aria-hidden="true" />
        <span class="hidden sm:inline">{$t('services.refresh')}</span>
      </Button>
      <Button variant="outline" size="sm" onclick={() => onchanged?.()}>
        <RefreshCwIcon aria-hidden="true" />
        <span class="hidden sm:inline">{$t('services.reload')}</span>
      </Button>
      <Button size="sm" onclick={openCreate}>
        <PlusIcon aria-hidden="true" />
        {$t('services.add')}
      </Button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="grid gap-4 p-4 sm:p-6">
      <div class="relative max-w-xs">
        <SearchIcon
          class="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          class="pl-8"
          placeholder={$t('services.searchNameOrUpstream')}
          aria-label={$t('services.searchServices')}
          bind:value={query}
        />
      </div>

      {#if loading}
        <SkeletonTable rows={3} columns={5} label={$t('loading.services')} />
      {:else if services.length === 0}
        <EmptyState
          icon={ServerIcon}
          title={$t('services.noServicesYet')}
          description={$t('services.aServiceIsA')}
        >
          {#snippet action()}
            <div class="grid w-full max-w-3xl gap-4">
              <div class="flex justify-center gap-2">
                <Button size="sm" onclick={openCreate}>
                  <PlusIcon aria-hidden="true" />
                  {$t('services.add')}
                </Button>
                <Button variant="outline" size="sm" onclick={() => onsetup?.()}>
                  {$t('wizard.open')}
                </Button>
              </div>
              <TemplatePicker class="text-left" onpicked={() => onnavigate?.('settings')} />
            </div>
          {/snippet}
        </EmptyState>
      {:else if filtered.length === 0}
        <EmptyState
          icon={SearchIcon}
          title={$t('services.nothingMatchesThatSearch')}
          description={$t('services.tryADifferentName')}
        />
      {:else}
        {#each filtered as service (service.name)}
          {@const live = statusByService.get(service.name)}
          {@const health = healthOf(service)}
          {@const using = routesUsing(service.name, routes)}
          <section
            class={cn(
              'rounded-xl border border-border bg-card',
              selectedServiceName === service.name && 'ring-2 ring-ring/50'
            )}
            data-service={service.name}
          >
            <div class="flex flex-wrap items-start justify-between gap-3 p-4">
              <div class="grid min-w-0 gap-1">
                <div class="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    class="rounded-sm text-base font-semibold hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
                    onclick={() => onselect?.(service.name)}
                  >
                    {service.name}
                  </button>
                  <StatusDot
                    status={serviceDot(service)}
                    reason={serviceReason(service)}
                    showLabel
                    tooltip={false}
                  />
                  <Badge variant="outline">{service.lb}</Badge>
                  {#if service.health_check.enabled}
                    <Badge variant="secondary">{$t('services.badge.probed')}</Badge>
                  {/if}
                  {#if service.sticky.enabled}
                    <Badge variant="secondary">{$t('services.badge.sticky')}</Badge>
                  {/if}
                  {#if live?.circuit_breaker_enabled}
                    <Badge variant="secondary">{$t('services.badge.breaker')}</Badge>
                  {/if}
                </div>
                <p class="text-xs text-muted-foreground">
                  {$t('services.meta.ready', { healthy: health.healthy, total: health.total })} ·
                  {$plural('services.meta.retries', service.max_retries)} ·
                  {#if using.length > 0}
                    {$t('services.meta.usedBy', {
                      routes: using.map((route) => route.name).join(', ')
                    })}
                  {:else}
                    {$t('services.meta.unused')}
                  {/if}
                </p>
              </div>

              <div class="flex shrink-0 items-center gap-1">
                <Button variant="outline" size="sm" onclick={() => onselect?.(service.name)}>
                  {$t('routes.action.edit')}
                </Button>
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger>
                    {#snippet child({ props }: { props: Record<string, unknown> })}
                      <Button
                        {...props}
                        variant="ghost"
                        size="icon-sm"
                        aria-label={$t('services.actionsFor', { name: service.name })}
                      >
                        <MoreHorizontalIcon aria-hidden="true" />
                      </Button>
                    {/snippet}
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Content align="end" class="w-52">
                    <DropdownMenu.Group>
                      <DropdownMenu.Item onSelect={() => onselect?.(service.name)}>
                        {$t('services.action.edit')}
                      </DropdownMenu.Item>
                      <DropdownMenu.Item onSelect={() => onnavigate?.('routes')}>
                        {$t('services.action.routes')}
                      </DropdownMenu.Item>
                    </DropdownMenu.Group>
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item
                      variant="destructive"
                      onSelect={() => askDelete(service.name)}
                    >
                      <Trash2Icon aria-hidden="true" />
                      {$t('common.delete')}
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
              </div>
            </div>

            <Table.Root>
              <Table.Header>
                <Table.Row>
                  <Table.Head>{$t('services.column.upstream')}</Table.Head>
                  <Table.Head class="text-right">{$t('services.column.share')}</Table.Head>
                  <Table.Head>{$t('services.column.state')}</Table.Head>
                  <Table.Head class="text-right">{$t('services.column.inflight')}</Table.Head>
                  <Table.Head class="text-right">{$t('services.column.latency')}</Table.Head>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {#each service.upstreams as upstream (upstream.addr)}
                  {@const liveUpstream = upstreamStatus(service.name)[upstream.addr]}
                  <Table.Row class={upstream.enabled ? '' : 'opacity-60'}>
                    <Table.Cell class="font-mono text-xs">
                      {upstream.addr}
                      {#if upstream.tls}
                        <Badge variant="outline" class="ml-1.5">TLS</Badge>
                      {/if}
                    </Table.Cell>
                    <Table.Cell class="text-right tabular-nums">
                      {#if upstream.enabled}
                        {liveUpstream ? `${(liveUpstream.share * 100).toFixed(0)}%` : '—'}
                      {:else}
                        {$t('services.drained')}
                      {/if}
                    </Table.Cell>
                    <Table.Cell>
                      <StatusDot
                        status={!upstream.enabled
                          ? 'disabled'
                          : !liveUpstream
                            ? 'unknown'
                            : liveUpstream.circuit_open
                              ? 'circuit-open'
                              : liveUpstream.probe_healthy
                                ? 'healthy'
                                : 'down'}
                        reason={!upstream.enabled
                          ? $t('upstreamEditor.reason.drained')
                          : liveUpstream?.circuit_open
                            ? liveUpstream.circuit_reopens_in_ms
                              ? $t('upstreamEditor.reason.circuitRetry', {
                                  failures: liveUpstream.consecutive_failures,
                                  seconds: Math.ceil(liveUpstream.circuit_reopens_in_ms / 1000)
                                })
                              : $t('upstreamEditor.reason.circuit', {
                                  failures: liveUpstream.consecutive_failures
                                })
                            : liveUpstream?.probe_healthy
                              ? $t('upstreamEditor.reason.healthy')
                              : $t('upstreamEditor.reason.down')}
                        showLabel
                        tooltip={false}
                      />
                    </Table.Cell>
                    <Table.Cell class="text-right tabular-nums">
                      {liveUpstream?.inflight ?? '—'}
                    </Table.Cell>
                    <Table.Cell class="text-right tabular-nums">
                      {liveUpstream && liveUpstream.ewma_us > 0
                        ? formatLatency(liveUpstream.ewma_us / 1000)
                        : '—'}
                    </Table.Cell>
                  </Table.Row>
                {/each}
              </Table.Body>
            </Table.Root>
          </section>
        {/each}
      {/if}
    </div>
  </div>
</div>

<ServiceSheet
  bind:open={sheetOpen}
  service={sheetService}
  mode={sheetMode}
  {services}
  {routes}
  status={sheetService ? upstreamStatus(sheetService.name) : {}}
  {saving}
  {serverError}
  onsave={save}
  oncancel={closeSheet}
  ondelete={(service) => askDelete(service.name)}
/>

<ConfirmDialog
  bind:open={confirmOpen}
  variant="destructive"
  title={$t('services.confirmDelete', { name: confirmTarget ?? '' })}
  description={$t('services.theUpstreamsBehindIt')}
  confirmLabel={$t('services.delete')}
  pending={saving}
  onconfirm={confirmDelete}
  oncancel={() => (confirmTarget = null)}
/>
