<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Button } from '$lib/components/ui/button';
  import { formatCount, formatLatency, formatPercent, formatThroughput } from '$lib/format';
  import type { RouteStats } from '$lib/api/stats';
  import { cn } from '$lib/utils';
  import { t } from '$lib/i18n';

  let {
    title,
    description,
    routes,
    /** `traffic` ranks by requests; `trouble` by error rate, then by p99. */
    rank,
    limit = 5,
    onselect,
    class: className
  }: {
    title: string;
    description: string;
    routes: RouteStats[];
    rank: 'traffic' | 'trouble';
    limit?: number;
    onselect?: (name: string) => void;
    class?: string;
  } = $props();

  const ranked = $derived.by(() => {
    const entries = routes.slice();
    if (rank === 'traffic') {
      entries.sort((a, b) => b.requests - a.requests);
      return entries.slice(0, limit);
    }
    // "Worst" means worst for the people using it: failing first, then slow.
    // A route with no errors and no latency to speak of is not a problem, so it
    // does not belong on this list at all.
    return entries
      .filter((route) => route.error_ratio > 0 || (route.p99_ms ?? 0) > 0)
      .sort(
        (a, b) =>
          b.error_ratio - a.error_ratio ||
          (b.p99_ms ?? 0) - (a.p99_ms ?? 0) ||
          b.requests - a.requests
      )
      .slice(0, limit);
  });

  const window = $derived(
    routes[0]?.window_seconds ? Math.round(routes[0].window_seconds) : 60
  );

  const errorTone = (ratio: number): string =>
    ratio >= 0.05
      ? 'text-destructive-emphasis'
      : ratio > 0
        ? 'text-warning-emphasis'
        : 'text-muted-foreground';
</script>

<section
  data-leaderboard={rank}
  class={cn('rounded-xl border border-border bg-card shadow-sm', className)}
>
  <header class="flex items-baseline justify-between gap-2 border-b border-border px-4 py-3">
    <div>
      <h2 class="text-sm font-semibold">{title}</h2>
      <p class="text-xs text-muted-foreground">{description}</p>
    </div>
    <span class="text-xs text-muted-foreground">
      {$t('dashboard.leaderboard.window', { seconds: window })}
    </span>
  </header>

  {#if ranked.length === 0}
    <p class="px-4 py-8 text-center text-sm text-muted-foreground">
      {rank === 'traffic'
        ? $t('dashboard.leaderboard.emptyTraffic')
        : $t('dashboard.leaderboard.emptyErrors')}
    </p>
  {:else}
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head>{$t('dashboard.leaderboard.route')}</Table.Head>
          <Table.Head class="text-right">{$t('dashboard.leaderboard.traffic')}</Table.Head>
          <Table.Head class="text-right">{$t('dashboard.leaderboard.errors')}</Table.Head>
          <Table.Head class="text-right">p99</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each ranked as route (route.name)}
          <Table.Row data-route={route.name}>
            <Table.Cell class="font-medium">
              {#if onselect}
                <Button
                  variant="link"
                  size="sm"
                  class="h-auto p-0 text-sm"
                  onclick={() => onselect(route.name)}
                >
                  {route.name}
                </Button>
              {:else}
                {route.name}
              {/if}
            </Table.Cell>
            <Table.Cell class="text-right tabular-nums">
              {formatThroughput(route.rps)}
              <span class="block text-xs text-muted-foreground">
                {$t('dashboard.leaderboard.requests', { count: formatCount(route.requests) })}
              </span>
            </Table.Cell>
            <Table.Cell class={cn('text-right tabular-nums', errorTone(route.error_ratio))}>
              {formatPercent(route.error_ratio)}
              <span class="block text-xs text-muted-foreground">
                {$t('dashboard.leaderboard.errorSplit', {
                  fivexx: formatCount(route.requests_5xx),
                  fourxx: formatCount(route.requests_4xx)
                })}
              </span>
            </Table.Cell>
            <Table.Cell class="text-right tabular-nums">
              {formatLatency(route.p99_ms)}
              <span class="block text-xs text-muted-foreground">
                p50 {formatLatency(route.p50_ms)}
              </span>
            </Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  {/if}
</section>
