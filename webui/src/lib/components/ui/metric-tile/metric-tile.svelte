<script lang="ts">
  import type { Component } from 'svelte';
  import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
  import Sparkline from './sparkline.svelte';
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
    history = [],
    class: className
  }: {
    label: string;
    value: string;
    unit?: string;
    icon?: Component;
    delta?: number | null;
    betterWhen?: 'higher' | 'lower' | 'neutral';
    deltaLabel?: string;
    history?: number[];
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
  </div>

  <div class="flex items-baseline gap-1.5">
    <span class="text-2xl font-semibold tabular-nums tracking-tight">{value}</span>
    {#if unit}
      <span class="text-sm text-muted-foreground">{unit}</span>
    {/if}
  </div>

  {#if delta !== null && delta !== undefined}
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

  {#if history.length > 1}
    <Sparkline values={history} class={tone === 'bad' ? 'text-destructive' : 'text-primary'} />
  {/if}
</div>
