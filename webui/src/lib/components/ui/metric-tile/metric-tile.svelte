<script lang="ts">
  import type { Component } from 'svelte';
  import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
  import InfoIcon from '@lucide/svelte/icons/info';
  import Sparkline from './sparkline.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { formatDelta } from '$lib/format';
  import { cn } from '$lib/utils';

  let {
    label,
    /** Pre-formatted through $lib/format — the tile does not invent units. */
    value,
    unit,
    icon,
    /** Change against the previous window, in the same unit as the value. */
    delta,
    /** Latency and error rate are better when they fall; throughput is not. */
    betterWhen = 'higher',
    deltaLabel = 'vs previous window',
    /**
     * Where the number comes from. A dashboard that cannot say which metric a
     * tile is reading is a dashboard nobody can act on, so this shows as an
     * info affordance next to the label.
     */
    hint,
    history = [],
    /** Before the first sample: the tile's own shape, without numbers in it. */
    loading = false,
    class: className
  }: {
    label: string;
    value: string;
    unit?: string;
    icon?: Component;
    delta?: number | null;
    betterWhen?: 'higher' | 'lower' | 'neutral';
    deltaLabel?: string;
    hint?: string;
    history?: number[];
    loading?: boolean;
    class?: string;
  } = $props();

  const Icon = $derived(icon);
  const direction = $derived(
    delta === null || delta === undefined || delta === 0 ? 'flat' : delta > 0 ? 'up' : 'down'
  );
  const tone = $derived(
    betterWhen === 'neutral' || direction === 'flat'
      ? 'neutral'
      : (direction === 'up') === (betterWhen === 'higher')
        ? 'good'
        : 'bad'
  );
  const DeltaIcon = $derived(
    direction === 'up' ? ArrowUpIcon : direction === 'down' ? ArrowDownIcon : ArrowRightIcon
  );
</script>

<div
  data-slot="metric-tile"
  class={cn(
    'flex flex-col gap-2 rounded-xl border border-border bg-card p-4 text-card-foreground shadow-sm',
    className
  )}
>
  <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
    {#if Icon}
      <Icon class="size-4 shrink-0" aria-hidden="true" />
    {/if}
    <span>{label}</span>
    {#if hint}
      <Tooltip.Root>
        <Tooltip.Trigger
          class="rounded-sm text-muted-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
          aria-label="{label}: where this number comes from"
        >
          <InfoIcon class="size-3.5" aria-hidden="true" />
        </Tooltip.Trigger>
        <Tooltip.Content class="max-w-64">{hint}</Tooltip.Content>
      </Tooltip.Root>
    {/if}
  </div>

  <div class="flex items-baseline gap-1.5" aria-busy={loading || undefined}>
    {#if loading}
      <!-- Same line height as the number it stands in for, so nothing moves
           when the first sample lands. -->
      <Skeleton class="my-1 h-6 w-20" />
    {:else}
      <span class="text-2xl font-semibold tabular-nums tracking-tight">{value}</span>
      {#if unit}
        <span class="text-sm text-muted-foreground">{unit}</span>
      {/if}
    {/if}
  </div>

  {#if delta !== null && delta !== undefined && !loading}
    <!-- The arrow says which way it moved and the sign says it again, so the
         colour is confirmation rather than the message. -->
    <p
      class={cn(
        'flex items-center gap-1 text-xs font-medium tabular-nums',
        tone === 'good' && 'text-success-emphasis',
        tone === 'bad' && 'text-destructive-emphasis',
        tone === 'neutral' && 'text-muted-foreground'
      )}
    >
      <DeltaIcon class="size-3.5 shrink-0" aria-hidden="true" />
      <span>{formatDelta(delta)}{unit ? ` ${unit}` : ''}</span>
      <span class="font-normal text-muted-foreground">{deltaLabel}</span>
    </p>
  {/if}

  {#if history.length > 1 && !loading}
    <Sparkline values={history} class={tone === 'bad' ? 'text-destructive' : 'text-primary'} />
  {/if}
</div>
