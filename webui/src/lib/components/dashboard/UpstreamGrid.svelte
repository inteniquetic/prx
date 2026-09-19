<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { STATUS_META, type UpstreamStatus } from '$lib/components/ui/status-dot';
  import { formatLatency } from '$lib/format';
  import type { ServiceHealth, UpstreamLiveState } from '$lib/api/stats';
  import { cn } from '$lib/utils';

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

  const reason = (service: ServiceHealth, state: UpstreamLiveState): string => {
    switch (state) {
      case 'healthy':
        return `Taking traffic for ${service.name}.`;
      case 'degraded':
        return `Still in the pool for ${service.name}, but failing some requests.`;
      case 'down':
        return `Out of the pool for ${service.name} — circuit open or failing health checks.`;
      default:
        return `Drained in the config, so ${service.name} sends it nothing.`;
    }
  };
</script>

<section class={cn('rounded-xl border border-border bg-card p-4 shadow-sm', className)}>
  <header class="mb-3 flex items-baseline justify-between gap-2">
    <div>
      <h3 class="text-sm font-semibold">Upstream health</h3>
      <p class="text-xs text-muted-foreground">
        One square per upstream, grouped by service. Click one to open its service.
      </p>
    </div>
    <span class="text-xs tabular-nums text-muted-foreground">{total} upstreams</span>
  </header>

  {#if services.length === 0}
    <p class="py-6 text-center text-sm text-muted-foreground">No services are configured.</p>
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
              {service.healthy}/{service.total} ready
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
                  aria-label="{upstream.addr}: {STATUS_META[status].label}"
                ></Tooltip.Trigger>
                <Tooltip.Content>
                  <span class="block font-medium">{upstream.addr}</span>
                  <span class="block">{STATUS_META[status].label} — {reason(service, upstream.state)}</span>
                  <span class="block text-tooltip-foreground/80">
                    {upstream.inflight} in flight · {formatLatency(upstream.ewma_ms)} avg
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
