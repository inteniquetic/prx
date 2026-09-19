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
  import { plural, t } from '$lib/i18n';

  let {
    config,
    onnavigate,
    onselectRoute,
    onselectService,
    onaddRoute,
    /** Opens the setup wizard, which lives in the shell. */
    onsetup
  }: {
    config: PrxConfig;
    onnavigate?: (page: 'routes' | 'services' | 'settings') => void;
    onselectRoute?: (name: string) => void;
    onselectService?: (name: string) => void;
    onaddRoute?: () => void;
    onsetup?: () => void;
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
          text: $t('banner.problem.allDown', { service: service.name }),
          action: { label: $t('banner.action.service'), go: () => onselectService?.(service.name) }
        });
      } else if (service.healthy < service.total) {
        found.push({
          text: $t('banner.problem.someDown', {
            service: service.name,
            healthy: service.healthy,
            total: service.total
          }),
          action: { label: $t('banner.action.service'), go: () => onselectService?.(service.name) }
        });
      }
    }

    if (sample.error_ratio_5xx >= 0.05) {
      const worst = live.routes
        .slice()
        .sort((a, b) => b.requests_5xx - a.requests_5xx)
        .find((route) => route.requests_5xx > 0);
      found.push({
        text: worst
          ? $t('banner.problem.errorsWorst', {
              percent: formatPercent(sample.error_ratio_5xx),
              route: worst.name
            })
          : $t('banner.problem.errors', { percent: formatPercent(sample.error_ratio_5xx) }),
        action: worst
          ? { label: $t('banner.action.route'), go: () => onselectRoute?.(worst.name) }
          : undefined
      });
    }

    for (const event of live.events) {
      if (event.kind === 'cert_expiry' && event.level !== 'info') {
        found.push({
          text: event.message,
          action: { label: $t('banner.action.settings'), go: () => onnavigate?.('settings') }
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

  const connectionNote = $derived($t(`dashboard.live.${live.status}`));
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">{$t('dashboard.title')}</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {$plural('dashboard.routeCount', routes.length)} ·
        {$plural('dashboard.serviceCount', services.length)} ·
        <span data-slot="live-status">{connectionNote}</span>
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => onnavigate?.('routes')}>
        <ActivityIcon aria-hidden="true" />
        <span class="hidden sm:inline">{$t('dashboard.allRoutes')}</span>
      </Button>
      <Button size="sm" onclick={() => onaddRoute?.()}>
        <PlusIcon aria-hidden="true" />
        {$t('dashboard.addRoute')}
      </Button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="grid gap-4 p-4 sm:p-6">
      {#if services.length === 0}
        <section
          class="rounded-xl border border-primary/40 bg-primary/5 p-5"
          data-slot="unconfigured"
        >
          <h2 class="text-sm font-semibold text-foreground">{$t('wizard.emptyTitle')}</h2>
          <p class="mt-1 max-w-2xl text-sm text-muted-foreground">{$t('wizard.emptyBody')}</p>
          <Button size="sm" class="mt-3" onclick={() => onsetup?.()}>{$t('wizard.open')}</Button>
        </section>
      {/if}

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
          loading={!live.loaded}
          label={$t('dashboard.tile.requests')}
          icon={GaugeIcon}
          value={formatThroughput(sample?.rps ?? null)}
          delta={delta((entry) => entry.rps)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.rps)}
          hint={$t('dashboard.tile.requestsHint')}
        />
        <MetricTile
          label={$t('dashboard.tile.p99')}
          icon={TimerIcon}
          value={formatLatency(sample?.p99_ms ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.p99_ms)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.p99_ms)}
          hint={$t('dashboard.tile.p99Hint')}
        />
        <MetricTile
          label={$t('dashboard.tile.fivexx')}
          icon={TriangleAlertIcon}
          value={formatPercent(sample?.error_ratio_5xx ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.error_ratio_5xx * 100)}
          deltaLabel="points vs a minute ago"
          history={spark((entry) => entry.error_ratio_5xx)}
          hint={$t('dashboard.tile.fivexxHint')}
        />
        <MetricTile
          label={$t('dashboard.tile.fourxx')}
          icon={TriangleAlertIcon}
          value={formatPercent(sample?.error_ratio_4xx ?? null)}
          betterWhen="lower"
          delta={delta((entry) => entry.error_ratio_4xx * 100)}
          deltaLabel="points vs a minute ago"
          history={spark((entry) => entry.error_ratio_4xx)}
          hint={$t('dashboard.tile.fourxxHint')}
        />
        <MetricTile
          label={$t('dashboard.tile.inflight')}
          icon={ActivityIcon}
          value={formatCount(sample?.inflight ?? null)}
          betterWhen="neutral"
          delta={delta((entry) => entry.inflight)}
          deltaLabel="vs a minute ago"
          history={spark((entry) => entry.inflight)}
          hint={$t('dashboard.tile.inflightHint')}
        />
        {#if cacheEnabled}
          <MetricTile
            label={$t('dashboard.tile.cache')}
            icon={DatabaseZapIcon}
            value={formatPercent(sample?.cache_hit_ratio ?? null)}
            delta={delta((entry) =>
              entry.cache_hit_ratio === null ? null : entry.cache_hit_ratio * 100
            )}
            deltaLabel="points vs a minute ago"
            history={spark((entry) => entry.cache_hit_ratio)}
            hint={$t('dashboard.tile.cacheHint')}
          />
        {:else}
          <MetricTile
            label={$t('dashboard.tile.upstreams')}
            icon={ServerIcon}
            value={sample ? `${sample.upstreams_healthy}/${sample.upstreams_total}` : '—'}
            betterWhen="higher"
            history={spark((entry) => entry.upstreams_healthy)}
            hint={$t('dashboard.tile.upstreamsHint')}
          />
        {/if}
      </section>

      <section class="grid gap-4 xl:grid-cols-2">
        <TimeseriesChart
          title={$t('dashboard.chart.rps')}
          description={$t('dashboard.chart.rpsHelp')}
          series={trafficSeries}
          {timestamps}
          mode="stacked"
          format={(value) => (value === null ? '—' : formatThroughput(value))}
          formatTick={compactRate}
          floor={1}
        />
        <TimeseriesChart
          title={$t('dashboard.chart.latency')}
          description={$t('dashboard.chart.latencyHelp')}
          series={latencySeries}
          {timestamps}
          format={(value) => (value === null ? '—' : formatLatency(value))}
          formatTick={compactMs}
          floor={10}
        />
      </section>

      <section class="grid gap-4 xl:grid-cols-2">
        <RouteLeaderboard
          title={$t('dashboard.top.title')}
          description={$t('dashboard.top.help')}
          routes={live.routes}
          rank="traffic"
          onselect={(name) => onselectRoute?.(name)}
        />
        <RouteLeaderboard
          title={$t('dashboard.worst.title')}
          description={$t('dashboard.worst.help')}
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

      <p class="text-xs text-muted-foreground">{$t('dashboard.footnote')}</p>
    </div>
  </div>
</div>
