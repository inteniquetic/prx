<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { STATUS_META, type UpstreamStatus } from '$lib/components/ui/status-dot';
  import { formatLatency } from '$lib/format';
  import type { ServiceHealth, UpstreamLiveState } from '$lib/api/stats';
  import { cn } from '$lib/utils';
  import { plural, t } from '$lib/i18n';

  let {
    services,
    onselect,
    class: className
  }: {
    services: ServiceHealth[];
    /** Clicking an upstream goes to the service that owns it. */
    onselect?: (service: string) => void;
    class?: string;
  } = $props();

  const dotStatus = (state: UpstreamLiveState): UpstreamStatus =>
    state === 'drained' ? 'disabled' : state;

  const total = $derived(
    services.reduce((sum, service) => sum + service.upstreams.length, 0)
  );

  const reason = (service: ServiceHealth, state: UpstreamLiveState): string =>
    $t(`dashboard.upstreams.reason.${state === 'drained' ? 'drained' : state}`, {
      service: service.name
    });
</script>

<section class={cn('rounded-xl border border-border bg-card p-4 shadow-sm', className)}>
  <header class="mb-3 flex items-baseline justify-between gap-2">
    <div>
      <h2 class="text-sm font-semibold">{$t('dashboard.upstreams.title')}</h2>
      <p class="text-xs text-muted-foreground">{$t('dashboard.upstreams.help')}</p>
    </div>
    <span class="text-xs tabular-nums text-muted-foreground">
      {$plural('dashboard.upstreams.count', total)}
    </span>
  </header>

  {#if services.length === 0}
    <p class="py-6 text-center text-sm text-muted-foreground">
      {$t('dashboard.upstreams.none')}
    </p>
  {:else}
    <ul class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {#each services as service (service.name)}
        <li class="rounded-lg border border-border/70 p-3" data-service={service.name}>
          <div class="mb-2 flex items-baseline justify-between gap-2">
            <button
              type="button"
              class="truncate rounded-sm text-sm font-medium hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
              onclick={() => onselect?.(service.name)}
            >
              {service.name}
            </button>
            <span
              class={cn(
                'shrink-0 text-xs tabular-nums',
                service.total > 0 && service.healthy === 0
                  ? 'text-destructive-emphasis'
                  : service.healthy < service.total
                    ? 'text-warning-emphasis'
                    : 'text-muted-foreground'
              )}
            >
              {$t('dashboard.upstreams.ready', {
                healthy: service.healthy,
                total: service.total
              })}
            </span>
          </div>

          <div class="flex flex-wrap gap-1.5">
            {#each service.upstreams as upstream (upstream.addr)}
              {@const status = dotStatus(upstream.state)}
              <Tooltip.Root>
                <Tooltip.Trigger
                  class={cn(
                    'size-5 rounded-[4px] focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none',
                    STATUS_META[status].dot,
                    // A drained upstream is a deliberate state, not a failure:
                    // hatching says "off" without shouting.
                    upstream.state === 'drained' && 'opacity-40'
                  )}
                  onclick={() => onselect?.(service.name)}
                  aria-label={$t('dashboard.upstreams.aria', {
                    addr: upstream.addr,
                    status: $t(STATUS_META[status].key)
                  })}
                ></Tooltip.Trigger>
                <Tooltip.Content>
                  <span class="block font-medium">{upstream.addr}</span>
                  <span class="block">
                    {$t(STATUS_META[status].key)} — {reason(service, upstream.state)}
                  </span>
                  <span class="block text-tooltip-foreground/80">
                    {$t('dashboard.upstreams.inflight', {
                      count: upstream.inflight,
                      latency: formatLatency(upstream.ewma_ms)
                    })}
                  </span>
                </Tooltip.Content>
              </Tooltip.Root>
            {/each}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>
