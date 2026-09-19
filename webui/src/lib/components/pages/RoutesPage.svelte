<script lang="ts">
  import ArrowUpDownIcon from '@lucide/svelte/icons/arrow-up-down';
  import ActivityIcon from '@lucide/svelte/icons/activity';
  import CopyIcon from '@lucide/svelte/icons/copy';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import EyeIcon from '@lucide/svelte/icons/eye';
  import FlaskConicalIcon from '@lucide/svelte/icons/flask-conical';
  import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import RouteIcon from '@lucide/svelte/icons/route';
  import SearchIcon from '@lucide/svelte/icons/search';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import * as Select from '$lib/components/ui/select';
  import * as Table from '$lib/components/ui/table';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { EmptyState } from '$lib/components/ui/empty-state';
  import { Input } from '$lib/components/ui/input';
  import { StatusDot, type UpstreamStatus } from '$lib/components/ui/status-dot';
  import { toast } from '$lib/components/ui/sonner';

  import RouteSheet from '../routes/RouteSheet.svelte';
  import RouteTester from '../routes/RouteTester.svelte';

  import { createRoute, deleteRoute, updateRoute, type RouteHealthItem } from '$lib/api/admin';
  import { analyzeRoutes, tierLabel, type RouteInsight } from '$lib/routeAnalysis';
  import { createDefaultRoute, type PrxConfig, type RouteConfig } from '$lib/types/config';
  import { cn } from '$lib/utils';

  let {
    config,
    /** The route the URL is pointing at — `/routes/:name`. */
    selectedRouteName = null,
    routeHealthByIndex = {},
    healthLoading = false,
    healthError = '',
    /** Bumped by the shell when something asks for a new route. */
    createRequest = 0,
    onselect,
    onchanged,
    onrefreshHealth,
    onnavigate
  }: {
    config: PrxConfig;
    selectedRouteName?: string | null;
    routeHealthByIndex?: Record<number, RouteHealthItem>;
    healthLoading?: boolean;
    healthError?: string;
    createRequest?: number;
    onselect?: (name: string | null) => void;
    onchanged?: () => void;
    onrefreshHealth?: () => void;
    onnavigate?: (page: 'services') => void;
  } = $props();

  type SortKey = 'order' | 'name' | 'host' | 'path' | 'service' | 'priority';

  let query = $state('');
  let serviceFilter = $state('all');
  let statusFilter = $state('all');
  let sortKey = $state<SortKey>('order');
  let sortAscending = $state(true);
  let page = $state(1);
  let pageSize = $state(25);
  let selection = $state<Set<string>>(new Set());
  let testerOpen = $state(false);
  /** The one row whose action menu is mounted, if any. */
  let openMenuFor = $state<string | null>(null);
  let testerHost = $state('');
  let testerPath = $state('/');

  let sheetOpen = $state(false);
  let sheetMode = $state<'create' | 'edit'>('edit');
  let sheetRoute = $state<RouteConfig | null>(null);
  let saving = $state(false);
  let serverError = $state<string | null>(null);

  let confirmOpen = $state(false);
  let confirmTargets = $state<string[]>([]);
  let busy = $state(false);

  const routes = $derived(config.routes ?? []);
  const services = $derived(config.services ?? []);

  // Analysed once per config, not once per keystroke: precedence and shadowing
  // depend on the whole list, and recomputing them while someone types is what
  // makes a 500-route table feel slow.
  const insights = $derived(analyzeRoutes(routes));
  const byName = $derived(new Map(insights.map((insight) => [insight.route.name, insight])));

  const healthStatus = (index: number): UpstreamStatus => {
    const route = routes[index];
    if (route && !route.enabled) return 'disabled';
    const health = routeHealthByIndex[index];
    if (!health) return 'unknown';
    if (health.healthy) return 'healthy';
    return health.reachable_upstreams > 0 ? 'degraded' : 'down';
  };

  const healthReason = (index: number): string => {
    const route = routes[index];
    if (route && !route.enabled) return 'Disabled: this route is not in the matcher.';
    const health = routeHealthByIndex[index];
    if (!health) return 'No probe has run since the last config load.';
    return `${health.reachable_upstreams} of ${health.total_upstreams} upstream${
      health.total_upstreams === 1 ? '' : 's'
    } reachable.`;
  };

  const matches = (insight: RouteInsight, needle: string): boolean =>
    !needle || insight.haystack.includes(needle);

  const matchesStatus = (insight: RouteInsight): boolean => {
    switch (statusFilter) {
      case 'all':
        return true;
      case 'warnings':
        return insight.warnings.some((warning) => warning.kind !== 'disabled');
      case 'disabled':
        return !insight.route.enabled;
      default:
        return healthStatus(insight.index) === statusFilter;
    }
  };

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return insights.filter(
      (insight) =>
        matches(insight, needle) &&
        (serviceFilter === 'all' || insight.route.service === serviceFilter) &&
        matchesStatus(insight)
    );
  });

  const sorted = $derived.by(() => {
    const direction = sortAscending ? 1 : -1;
    const value = (insight: RouteInsight): string | number => {
      switch (sortKey) {
        case 'name':
          return insight.route.name;
        case 'host':
          return insight.route.host || '￿'; // "any host" sorts last
        case 'path':
          return insight.route.path_prefix;
        case 'service':
          return insight.route.service;
        case 'priority':
          return insight.precedence;
        default:
          return insight.index;
      }
    };

    return [...filtered].sort((a, b) => {
      const left = value(a);
      const right = value(b);
      if (typeof left === 'number' && typeof right === 'number') {
        return (left - right) * direction;
      }
      return String(left).localeCompare(String(right)) * direction;
    });
  });

  const pageCount = $derived(Math.max(1, Math.ceil(sorted.length / pageSize)));
  const currentPage = $derived(Math.min(page, pageCount));
  const visible = $derived(
    sorted.slice((currentPage - 1) * pageSize, (currentPage - 1) * pageSize + pageSize)
  );

  const allVisibleSelected = $derived(
    visible.length > 0 && visible.every((insight) => selection.has(insight.route.name))
  );

  const withWarnings = $derived(
    insights.filter((insight) => insight.warnings.some((warning) => warning.kind !== 'disabled'))
      .length
  );

  // Anything that changes the list also changes which page makes sense.
  $effect(() => {
    void query;
    void serviceFilter;
    void statusFilter;
    page = 1;
  });

  function toggleSort(key: SortKey) {
    if (sortKey === key) sortAscending = !sortAscending;
    else {
      sortKey = key;
      sortAscending = true;
    }
  }

  function toggleSelection(name: string, checked: boolean) {
    const next = new Set(selection);
    if (checked) next.add(name);
    else next.delete(name);
    selection = next;
  }

  function toggleAllVisible(checked: boolean) {
    const next = new Set(selection);
    for (const insight of visible) {
      if (checked) next.add(insight.route.name);
      else next.delete(insight.route.name);
    }
    selection = next;
  }

  // --- the sheet ----------------------------------------------------------

  function openCreate() {
    const base = createDefaultRoute(routes.length + 1, services[0]?.name);
    base.is_default = false;
    // A new route should not collide with an existing name on the first try.
    let candidate = base.name;
    let suffix = routes.length + 1;
    while (routes.some((route) => route.name === candidate)) {
      suffix += 1;
      candidate = `route-${suffix}`;
    }
    base.name = candidate;
    sheetRoute = base;
    sheetMode = 'create';
    serverError = null;
    sheetOpen = true;
  }

  function openEdit(name: string) {
    const route = routes.find((entry) => entry.name === name);
    if (!route) return;
    sheetRoute = route;
    sheetMode = 'edit';
    serverError = null;
    sheetOpen = true;
  }

  // The URL owns which route is open, so the sheet follows it rather than the
  // other way round (T303).
  $effect(() => {
    const name = selectedRouteName;
    if (!name) {
      if (sheetMode === 'edit' && sheetOpen) sheetOpen = false;
      return;
    }
    if (routes.length === 0) return;
    const route = routes.find((entry) => entry.name === name);
    if (!route) return;
    sheetRoute = route;
    sheetMode = 'edit';
    sheetOpen = true;
  });

  // Starts where the shell starts, so mounting never counts as a request.
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
    if (selectedRouteName) onselect?.(null);
  }

  async function saveRoute(route: RouteConfig) {
    saving = true;
    serverError = null;
    try {
      if (sheetMode === 'create') {
        await createRoute(route);
        toast.success(`Route “${route.name}” created`);
      } else {
        const original = sheetRoute?.name ?? route.name;
        await updateRoute(original, route);
        toast.success(`Route “${route.name}” saved`);
      }
      sheetOpen = false;
      if (selectedRouteName) onselect?.(null);
      onchanged?.();
    } catch (err) {
      // Straight from the admin API; the sheet decides which field it belongs to.
      serverError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  // --- row and bulk actions ------------------------------------------------

  async function duplicate(name: string) {
    const route = routes.find((entry) => entry.name === name);
    if (!route || busy) return;
    const copy = JSON.parse(JSON.stringify(route)) as RouteConfig;
    let candidate = `${route.name}-copy`;
    let n = 1;
    while (routes.some((entry) => entry.name === candidate)) {
      n += 1;
      candidate = `${route.name}-copy-${n}`;
    }
    copy.name = candidate;
    // Two default routes is a config the proxy refuses, so the copy is not one.
    copy.is_default = false;

    busy = true;
    try {
      await createRoute(copy);
      toast.success(`Duplicated as “${candidate}”`);
      onchanged?.();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function setEnabled(names: string[], enabled: boolean) {
    if (busy || names.length === 0) return;
    busy = true;
    let changed = 0;
    const failures: string[] = [];

    for (const name of names) {
      const route = routes.find((entry) => entry.name === name);
      if (!route || route.enabled === enabled) continue;
      try {
        await updateRoute(name, { ...route, enabled });
        changed += 1;
      } catch (err) {
        failures.push(`${name}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    busy = false;
    if (changed > 0) {
      toast.success(`${changed} route${changed === 1 ? '' : 's'} ${enabled ? 'enabled' : 'disabled'}`);
      onchanged?.();
    }
    for (const failure of failures) toast.error(failure);
  }

  function askDelete(names: string[]) {
    if (names.length === 0) return;
    confirmTargets = names;
    confirmOpen = true;
  }

  async function confirmDelete() {
    if (busy) return;
    busy = true;
    const failures: string[] = [];
    let removed = 0;

    for (const name of confirmTargets) {
      try {
        await deleteRoute(name);
        removed += 1;
      } catch (err) {
        failures.push(`${name}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    busy = false;
    confirmOpen = false;
    selection = new Set();
    if (removed > 0) {
      toast.success(`${removed} route${removed === 1 ? '' : 's'} deleted`);
      if (selectedRouteName && confirmTargets.includes(selectedRouteName)) onselect?.(null);
      onchanged?.();
    }
    for (const failure of failures) toast.error(failure);
    confirmTargets = [];
  }

  function openTesterFor(insight: RouteInsight) {
    testerHost = insight.route.host || '';
    testerPath = insight.route.path_prefix;
    testerOpen = true;
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">Routes</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {routes.length} route{routes.length === 1 ? '' : 's'}, matched most specific first
        {#if withWarnings > 0}
          · <span class="text-warning-emphasis">{withWarnings} need attention</span>
        {/if}
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => (testerOpen = !testerOpen)}>
        <FlaskConicalIcon aria-hidden="true" />
        <span class="hidden sm:inline">Test a request</span>
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={healthLoading}
        onclick={() => onrefreshHealth?.()}
      >
        <ActivityIcon aria-hidden="true" class={healthLoading ? 'animate-spin' : ''} />
        <span class="hidden sm:inline">{healthLoading ? 'Checking...' : 'Check health'}</span>
      </Button>
      <Button variant="outline" size="sm" onclick={() => onchanged?.()}>
        <RefreshCwIcon aria-hidden="true" />
        <span class="hidden sm:inline">Reload</span>
      </Button>
      <Button size="sm" onclick={openCreate}>
        <PlusIcon aria-hidden="true" />
        Add route
      </Button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="grid gap-4 p-4 sm:p-6">
      {#if testerOpen}
        <section class="rounded-xl border border-border bg-card p-4">
          <div class="mb-3 flex items-center justify-between gap-2">
            <div>
              <h2 class="text-sm font-semibold">Route tester</h2>
              <p class="text-xs text-muted-foreground">
                Runs through the proxy's own matcher, against the config it is serving right now.
              </p>
            </div>
            <Button variant="ghost" size="sm" onclick={() => (testerOpen = false)}>Close</Button>
          </div>
          <RouteTester
            bind:host={testerHost}
            bind:path={testerPath}
            onopenRoute={(name) => onselect?.(name)}
          />
        </section>
      {/if}

      {#if healthError}
        <p class="text-sm text-destructive-emphasis">{healthError}</p>
      {/if}

      <!-- Filters --------------------------------------------------------- -->
      <div class="flex flex-wrap items-center gap-2">
        <div class="relative min-w-0 flex-1 sm:max-w-xs">
          <SearchIcon
            class="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
            aria-hidden="true"
          />
          <Input
            class="pl-8"
            placeholder="Search name, host, path, service"
            aria-label="Search routes"
            bind:value={query}
          />
        </div>

        <Select.Root type="single" bind:value={serviceFilter}>
          <Select.Trigger size="sm" class="w-40" aria-label="Filter by service">
            {serviceFilter === 'all' ? 'All services' : serviceFilter}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="all" label="All services" />
            {#each services as service (service.name)}
              <Select.Item value={service.name} label={service.name} />
            {/each}
          </Select.Content>
        </Select.Root>

        <Select.Root type="single" bind:value={statusFilter}>
          <Select.Trigger size="sm" class="w-40" aria-label="Filter by status">
            {statusFilter === 'all' ? 'Any status' : statusFilter}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="all" label="Any status" />
            <Select.Item value="healthy" label="Healthy" />
            <Select.Item value="degraded" label="Degraded" />
            <Select.Item value="down" label="Down" />
            <Select.Item value="disabled" label="Disabled" />
            <Select.Item value="warnings" label="Has warnings" />
          </Select.Content>
        </Select.Root>

        <span class="ml-auto text-xs text-muted-foreground" data-testid="result-count">
          {sorted.length} of {routes.length}
        </span>
      </div>

      <!-- Bulk actions ---------------------------------------------------- -->
      {#if selection.size > 0}
        <div
          class="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-accent/50 px-3 py-2"
        >
          <span class="text-sm font-medium">{selection.size} selected</span>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onclick={() => setEnabled([...selection], true)}
          >
            <EyeIcon aria-hidden="true" />
            Enable
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onclick={() => setEnabled([...selection], false)}
          >
            <EyeOffIcon aria-hidden="true" />
            Disable
          </Button>
          <Button
            variant="destructive"
            size="sm"
            disabled={busy}
            onclick={() => askDelete([...selection])}
          >
            <Trash2Icon aria-hidden="true" />
            Delete
          </Button>
          <Button variant="ghost" size="sm" onclick={() => (selection = new Set())}>Clear</Button>
        </div>
      {/if}

      <!-- Table ----------------------------------------------------------- -->
      {#if routes.length === 0}
        <EmptyState
          icon={RouteIcon}
          title="No routes yet"
          description="A route decides which requests reach which service. Add one to start sending traffic."
        >
          {#snippet action()}
            <Button size="sm" onclick={openCreate}>
              <PlusIcon aria-hidden="true" />
              Add route
            </Button>
          {/snippet}
        </EmptyState>
      {:else if sorted.length === 0}
        <EmptyState
          icon={SearchIcon}
          title="Nothing matches those filters"
          description="Try a different search, or clear the service and status filters."
        >
          {#snippet action()}
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                query = '';
                serviceFilter = 'all';
                statusFilter = 'all';
              }}
            >
              Clear filters
            </Button>
          {/snippet}
        </EmptyState>
      {:else}
        <div class="rounded-xl border border-border bg-card">
          <Table.Root>
            <Table.Header>
              <Table.Row>
                <Table.Head class="w-10">
                  <Checkbox
                    checked={allVisibleSelected}
                    aria-label="Select every route on this page"
                    onCheckedChange={(checked) => toggleAllVisible(checked)}
                  />
                </Table.Head>
                {#each [['name', 'Name'], ['host', 'Host'], ['path', 'Path prefix'], ['service', 'Service'], ['priority', 'Precedence']] as [key, label] (key)}
                  <Table.Head>
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-sm hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
                      onclick={() => toggleSort(key as SortKey)}
                      aria-label={`Sort by ${label}`}
                    >
                      {label}
                      <ArrowUpDownIcon
                        class={cn('size-3', sortKey === key ? 'opacity-100' : 'opacity-40')}
                        aria-hidden="true"
                      />
                    </button>
                  </Table.Head>
                {/each}
                <Table.Head>Status</Table.Head>
                <Table.Head class="w-10"><span class="sr-only">Actions</span></Table.Head>
              </Table.Row>
            </Table.Header>

            <Table.Body>
              {#each visible as insight (insight.route.name)}
                {@const route = insight.route}
                {@const warning = insight.warnings.find((entry) => entry.kind !== 'disabled')}
                <Table.Row
                  class={cn(
                    'cursor-pointer',
                    !route.enabled && 'opacity-60',
                    selectedRouteName === route.name && 'bg-accent/60'
                  )}
                  data-route={route.name}
                  onclick={() => onselect?.(route.name)}
                >
                  <Table.Cell onclick={(event: MouseEvent) => event.stopPropagation()}>
                    <input
                      type="checkbox"
                      class="size-4 accent-primary"
                      checked={selection.has(route.name)}
                      aria-label={`Select ${route.name}`}
                      onchange={(event) =>
                        toggleSelection(route.name, event.currentTarget.checked)}
                    />
                  </Table.Cell>

                  <Table.Cell>
                    <div class="flex items-center gap-1.5">
                      <!-- The row is clickable for the mouse, but the name is
                           the focusable control: a keyboard has to be able to
                           open a route without a pointer. -->
                      <button
                        type="button"
                        class="rounded-sm font-medium hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
                        onclick={(event: MouseEvent) => {
                          event.stopPropagation();
                          onselect?.(route.name);
                        }}
                      >
                        {route.name}
                      </button>
                      {#if route.is_default}
                        <Badge variant="outline">default</Badge>
                      {/if}
                      {#if !route.enabled}
                        <Badge variant="secondary">disabled</Badge>
                      {/if}
                      {#if warning}
                        <Tooltip.Root>
                          <Tooltip.Trigger class="rounded-sm focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none">
                            <TriangleAlertIcon
                              class="size-4 text-warning-emphasis"
                              aria-label="This route has a warning"
                            />
                          </Tooltip.Trigger>
                          <Tooltip.Content class="max-w-xs">{warning.message}</Tooltip.Content>
                        </Tooltip.Root>
                      {/if}
                    </div>
                  </Table.Cell>

                  <Table.Cell class="font-mono text-xs">
                    {#if route.host}
                      {route.host}
                    {:else}
                      <span class="text-muted-foreground">any</span>
                    {/if}
                  </Table.Cell>
                  <Table.Cell class="font-mono text-xs">{route.path_prefix}</Table.Cell>
                  <Table.Cell>
                    <!-- A plain button, not the Button component: this one is
                         drawn once per row and the table is redrawn on every
                         keystroke. -->
                    <button
                      type="button"
                      class="rounded-sm text-primary underline-offset-4 hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
                      onclick={(event: MouseEvent) => {
                        event.stopPropagation();
                        onnavigate?.('services');
                      }}
                    >
                      {route.service}
                    </button>
                  </Table.Cell>

                  <Table.Cell>
                    <span
                      class="text-xs text-muted-foreground"
                      title={`Exact host beats wildcard beats any host; inside one host the longest path prefix wins; ties go to whichever route comes first in the config. This one is #${insight.index + 1}.`}
                    >
                      {tierLabel[insight.tier]}
                      {#if route.methods.length > 0}
                        · {route.methods.join(', ')}
                      {/if}
                    </span>
                  </Table.Cell>

                  <Table.Cell>
                    <StatusDot
                      status={healthStatus(insight.index)}
                      reason={healthReason(insight.index)}
                      tooltip={false}
                    />
                  </Table.Cell>

                  <Table.Cell onclick={(event: MouseEvent) => event.stopPropagation()}>
                    {#if openMenuFor !== route.name}
                      <!-- The menu is mounted only for the row being acted on.
                           A dropdown per row is the single most expensive thing
                           in this table, and the table is rebuilt on every
                           keystroke in the search box. -->
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Actions for ${route.name}`}
                        onclick={() => (openMenuFor = route.name)}
                      >
                        <MoreHorizontalIcon aria-hidden="true" />
                      </Button>
                    {:else}
                    <DropdownMenu.Root
                      open
                      onOpenChange={(open) => {
                        if (!open) openMenuFor = null;
                      }}
                    >
                      <DropdownMenu.Trigger>
                        {#snippet child({ props }: { props: Record<string, unknown> })}
                          <Button
                            {...props}
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Actions for ${route.name}`}
                          >
                            <MoreHorizontalIcon aria-hidden="true" />
                          </Button>
                        {/snippet}
                      </DropdownMenu.Trigger>
                      <DropdownMenu.Content align="end" class="w-48">
                        <DropdownMenu.Group>
                          <DropdownMenu.Item onSelect={() => onselect?.(route.name)}>
                            Edit
                          </DropdownMenu.Item>
                          <DropdownMenu.Item onSelect={() => openTesterFor(insight)}>
                            <FlaskConicalIcon aria-hidden="true" />
                            Test this route
                          </DropdownMenu.Item>
                          <DropdownMenu.Item onSelect={() => duplicate(route.name)}>
                            <CopyIcon aria-hidden="true" />
                            Duplicate
                          </DropdownMenu.Item>
                          <DropdownMenu.Item
                            onSelect={() => setEnabled([route.name], !route.enabled)}
                          >
                            {#if route.enabled}
                              <EyeOffIcon aria-hidden="true" />
                              Disable
                            {:else}
                              <EyeIcon aria-hidden="true" />
                              Enable
                            {/if}
                          </DropdownMenu.Item>
                        </DropdownMenu.Group>
                        <DropdownMenu.Separator />
                        <DropdownMenu.Item
                          variant="destructive"
                          onSelect={() => askDelete([route.name])}
                        >
                          <Trash2Icon aria-hidden="true" />
                          Delete
                        </DropdownMenu.Item>
                      </DropdownMenu.Content>
                    </DropdownMenu.Root>
                    {/if}
                  </Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        </div>

        <!-- Pagination ---------------------------------------------------- -->
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2 text-xs text-muted-foreground">
            <span>Rows per page</span>
            <Select.Root
              type="single"
              value={String(pageSize)}
              onValueChange={(value) => {
                pageSize = Number(value);
                page = 1;
              }}
            >
              <Select.Trigger size="sm" class="w-20" aria-label="Rows per page">
                {pageSize}
              </Select.Trigger>
              <Select.Content>
                {#each [10, 25, 50, 100] as size (size)}
                  <Select.Item value={String(size)} label={String(size)} />
                {/each}
              </Select.Content>
            </Select.Root>
          </div>

          <div class="flex items-center gap-2">
            <span class="text-xs text-muted-foreground">
              Page {currentPage} of {pageCount}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={currentPage <= 1}
              onclick={() => (page = currentPage - 1)}
            >
              Previous
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={currentPage >= pageCount}
              onclick={() => (page = currentPage + 1)}
            >
              Next
            </Button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<RouteSheet
  bind:open={sheetOpen}
  route={sheetRoute}
  mode={sheetMode}
  {routes}
  {services}
  {saving}
  {serverError}
  onsave={saveRoute}
  oncancel={closeSheet}
  oncreateService={() => onnavigate?.('services')}
/>

<ConfirmDialog
  bind:open={confirmOpen}
  variant="destructive"
  title={confirmTargets.length === 1
    ? `Delete “${confirmTargets[0]}”?`
    : `Delete ${confirmTargets.length} routes?`}
  description="Traffic that matched them starts falling through to the next route, or to the fallback."
  confirmLabel="Delete"
  pending={busy}
  onconfirm={confirmDelete}
  oncancel={() => (confirmTargets = [])}
/>
