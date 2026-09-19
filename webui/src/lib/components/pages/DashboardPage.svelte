<script lang="ts">
  import { onMount } from 'svelte';

  import ActivityIcon from '@lucide/svelte/icons/activity';
  import DatabaseZapIcon from '@lucide/svelte/icons/database-zap';
  import GaugeIcon from '@lucide/svelte/icons/gauge';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import ServerIcon from '@lucide/svelte/icons/server';
  import TimerIcon from '@lucide/svelte/icons/timer';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { Button } from '$lib/components/ui/button';
  import { MetricTile } from '$lib/components/ui/metric-tile';

  import EventStrip from '../dashboard/EventStrip.svelte';
  import RouteLeaderboard from '../dashboard/RouteLeaderboard.svelte';
  import StatusBanner, { type SystemProblem, type SystemState } from '../dashboard/StatusBanner.svelte';
  import TimeseriesChart, { type ChartSeries } from '../dashboard/TimeseriesChart.svelte';
  import UpstreamGrid from '../dashboard/UpstreamGrid.svelte';

  import type { StatsSample } from '$lib/api/stats';
  import { formatCount, formatLatency, formatPercent, formatThroughput } from '$lib/format';
  import { liveStats, startLiveStats } from '$lib/stores/stats';
  import type { PrxConfig } from '$lib/types/config';

  let {
    config,
    onnavigate,
    onselectRoute,
    onselectService,
    onaddRoute
  }: {
    config: PrxConfig;
    onnavigate?: (page: 'routes' | 'services' | 'settings') => void;
    onselectRoute?: (name: string) => void;
    onselectService?: (name: string) => void;
    onaddRoute?: () => void;
  } = $props();

  // The stream lives exactly as long as this page is on screen: leaving the
  // dashboard closes it, which is what keeps the server's 16 slots free for
  // tabs that are actually looking.
  onMount(() => startLiveStats());

  const live = $derived($liveStats);
  const sample = $derived(live.sample);
  const history = $derived(live.history);

  // --- what the numbers mean --------------------------------------------------

  const cacheEnabled = $derived(
    (config.routes ?? []).some((route) => route.cache?.enabled === true)
  );

  /** The sample from about a minute ago, which is what each delta compares to. */
  const previous = $derived<StatsSample | null>(
    history.length > 60 ? history[history.length - 61] : null
  );

  const delta = (pick: (entry: StatsSample) => number | null): number | null => {
    if (!sample || !previous) return null;
    const now = pick(sample);
    const before = pick(previous);
    if (now === null || before === null) return null;
    return now - before;
  };

  /** Sparkline data: gaps are dropped rather than drawn as zero. */
  const spark = (pick: (entry: StatsSample) => number | null): number[] => {
    const values: number[] = [];
    for (const entry of history) {
      const value = pick(entry);
      if (value !== null && Number.isFinite(value)) values.push(value);
    }
    return values;
  };

  const timestamps = $derived(history.map((entry) => entry.epoch_ms));

  const trafficSeries = $derived<ChartSeries[]>([
    {
      key: '2xx',
      label: '2xx',
      color: 'var(--success)',
      values: history.map((entry) => entry.rps_2xx)
    },
    {
      key: '3xx',
      label: '3xx',
      color: 'var(--muted-foreground)',
      values: history.map((entry) => entry.rps_3xx)
    },
    {
      key: '4xx',
      label: '4xx',
      color: 'var(--warning)',
      values: history.map((entry) => entry.rps_4xx)
    },
    {
      key: '5xx',
      label: '5xx',
      color: 'var(--destructive)',
      values: history.map((entry) => entry.rps_5xx)
    }
  ]);

  // One hue, light to dark, plus a dash pattern: the three percentiles stay
  // apart without relying on three different colours.
  const latencySeries = $derived<ChartSeries[]>([
    {
      key: 'p50',
      label: 'p50',
      color: 'color-mix(in oklab, var(--primary) 62%, var(--card))',
      dash: '2 3',
      values: history.map((entry) => entry.p50_ms)
    },
    {
      key: 'p95',
      label: 'p95',
      color: 'color-mix(in oklab, var(--primary) 82%, var(--card))',
      dash: '6 3',
      values: history.map((entry) => entry.p95_ms)
    },
    {
      key: 'p99',
      label: 'p99',
      color: 'var(--primary)',
      values: history.map((entry) => entry.p99_ms)
    }
  ]);

  // --- is anything wrong ------------------------------------------------------

  const systemState = $derived<SystemState>(
    !live.loaded || !sample
      ? 'unknown'
      : sample.upstreams_total > 0 && sample.upstreams_healthy === 0
        ? 'down'
        : sample.upstreams_healthy < sample.upstreams_total || sample.error_ratio_5xx >= 0.05
          ? 'degraded'
          : 'ok'
  );

  const problems = $derived.by(() => {
    const found: SystemProblem[] = [];
    if (!sample) return found;

    for (const service of live.services) {
      if (service.total === 0) continue;
      if (service.healthy === 0) {
        found.push({
          text: `Every upstream in “${service.name}” is out of the pool.`,
          action: { label: 'Open service', go: () => onselectService?.(service.name) }
        });
      } else if (service.healthy < service.total) {
        found.push({
          text: `“${service.name}” is down to ${service.healthy} of ${service.total} upstreams.`,
          action: { label: 'Open service', go: () => onselectService?.(service.name) }
        });
      }
    }

    if (sample.error_ratio_5xx >= 0.05) {
      const worst = live.routes
        .slice()
        .sort((a, b) => b.requests_5xx - a.requests_5xx)
        .find((route) => route.requests_5xx > 0);
      found.push({
        text: `${formatPercent(sample.error_ratio_5xx)} of requests are answering 5xx${
          worst ? `, worst on “${worst.name}”` : ''
        }.`,
        action: worst
          ? { label: 'Open route', go: () => onselectRoute?.(worst.name) }
          : undefined
      });
    }

    for (const event of live.events) {
      if (event.kind === 'cert_expiry' && event.level !== 'info') {
        found.push({
          text: event.message,
          action: { label: 'Open settings', go: () => onnavigate?.('settings') }
        });
        break;
      }
    }

    return found;
  });

  /** Axis ticks carry no unit: the title and legend already said it. */
  const compactRate = (value: number): string =>
    value >= 1000 ? `${(value / 1000).toFixed(value >= 10000 ? 0 : 1)}k` : `${Math.round(value)}`;
  const compactMs = (value: number): string =>
    value >= 1000 ? `${(value / 1000).toFixed(1)}s` : `${Math.round(value)}`;

  const routes = $derived(config.routes ?? []);
  const services = $derived(config.services ?? []);

  const connectionNote = $derived(
    live.status === 'live'
      ? 'live'
      : live.status === 'polling'
        ? 'polling'
        : live.status === 'connecting'
          ? 'connecting'
          : live.status === 'reconnecting'
            ? 'reconnecting'
            : 'offline'
  );
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">Dashboard</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {routes.length} route{routes.length === 1 ? '' : 's'} ·
        {services.length} service{services.length === 1 ? '' : 's'} ·
        <span data-slot="live-status">{connectionNote}</span>
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => onnavigate?.('routes')}>
        <ActivityIcon aria-hidden="true" />
        <span class="hidden sm:inline">All routes</span>
      </Button>
      <Button size="sm" onclick={() => onaddRoute?.()}>
        <PlusIcon aria-hidden="true" />
        Add route
      </Button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="grid gap-4 p-4 sm:p-6">
      <StatusBanner
        state={systemState}
        {problems}
        connection={live.status}
        connectionError={live.error}
      />

      <!-- Every tile says where its number comes from, because "error rate"
           means four different things depending on what you counted. -->
      <section class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-6">
        <MetricTile
          label="Requests"
          icon={GaugeIcon}
          value={formatThroughput(sample?.rps ?? null)}
          delta={delta((entry) => entry.rps)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.rps)}
          hint="Requests finished per second, from prx_requests_total across every route."
        />
        <MetricTile
          label="p99 latency"
          icon={TimerIcon}
          value={formatLatency(sample?.p99_ms ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.p99_ms)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.p99_ms)}
          hint="99th percentile over this second, interpolated from the prx_request_latency_ms histogram."
        />
        <MetricTile
          label="5xx rate"
          icon={TriangleAlertIcon}
          value={formatPercent(sample?.error_ratio_5xx ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.error_ratio_5xx * 100)}
          deltaLabel="points vs a minute ago"
          history={spark((entry) => entry.error_ratio_5xx)}
          hint="Share of requests answering 5xx, from prx_requests_total split by status class. 4xx is counted separately."
        />
        <MetricTile
          label="4xx rate"
          icon={TriangleAlertIcon}
          value={formatPercent(sample?.error_ratio_4xx ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.error_ratio_4xx * 100)}
          deltaLabel="points vs a minute ago"
          history={spark((entry) => entry.error_ratio_4xx)}
          hint="Share of requests answering 4xx — usually the client's doing, not the proxy's."
        />
        <MetricTile
          label="In flight"
          icon={ActivityIcon}
          value={formatCount(sample?.inflight ?? null)}
          betterWhen="neutral"
          delta={delta((entry) => entry.inflight)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.inflight)}
          hint="Requests being served right now, from the prx_inflight_requests gauge."
        />
        {#if cacheEnabled}
          <MetricTile
            label="Cache hits"
            icon={DatabaseZapIcon}
            value={formatPercent(sample?.cache_hit_ratio ?? null)}
            delta={delta((entry) =>
              entry.cache_hit_ratio === null ? null : entry.cache_hit_ratio * 100
            )}
            deltaLabel="points vs a minute ago"
            history={spark((entry) => entry.cache_hit_ratio)}
            hint="Hits as a share of cache lookups in this second, from prx_cache_total."
          />
        {:else}
          <MetricTile
            label="Upstreams ready"
            icon={ServerIcon}
            value={sample ? `${sample.upstreams_healthy}/${sample.upstreams_total}` : '—'}
            betterWhen="higher"
            history={spark((entry) => entry.upstreams_healthy)}
            hint="Upstreams taking traffic, counted from the running config: drained ones are left out, circuit-open and failing ones count as not ready."
          />
        {/if}
      </section>

      <section class="grid gap-4 xl:grid-cols-2">
        <TimeseriesChart
          title="Requests per second"
          description="Stacked by status class, so the error share is the coloured band."
          series={trafficSeries}
          {timestamps}
          mode="stacked"
          format={(value) => (value === null ? '—' : formatThroughput(value))}
          formatTick={compactRate}
          floor={1}
        />
        <TimeseriesChart
          title="Latency percentiles"
          description="From the request histogram — a gap means no request finished that second."
          series={latencySeries}
          {timestamps}
          format={(value) => (value === null ? '—' : formatLatency(value))}
          formatTick={compactMs}
          floor={10}
        />
      </section>

      <section class="grid gap-4 xl:grid-cols-2">
        <RouteLeaderboard
          title="Top routes"
          description="Most traffic in the last minute."
          routes={live.routes}
          rank="traffic"
          onselect={(name) => onselectRoute?.(name)}
        />
        <RouteLeaderboard
          title="Worst routes"
          description="Highest error rate, then slowest p99."
          routes={live.routes}
          rank="trouble"
          onselect={(name) => onselectRoute?.(name)}
        />
      </section>

      <section class="grid gap-4 xl:grid-cols-2">
        <UpstreamGrid
          services={live.services}
          onselect={(name) => onselectService?.(name)}
        />
        <EventStrip events={live.events} />
      </section>

      <p class="text-xs text-muted-foreground">
        Live numbers cover the last five minutes and are held in memory only —
        restarting prx starts them over. For anything longer, scrape
        <code class="rounded bg-muted px-1">/metrics</code>.
      </p>
    </div>
  </div>
</div>
