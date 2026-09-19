<script lang="ts">
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import FileCogIcon from '@lucide/svelte/icons/file-cog';
  import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import ZapOffIcon from '@lucide/svelte/icons/zap-off';

  import type { StatsEvent } from '$lib/api/stats';
  import { cn } from '$lib/utils';

  let {
    events,
    limit = 8,
    class: className
  }: {
    events: StatsEvent[];
    limit?: number;
    class?: string;
  } = $props();

  const shown = $derived(events.slice(0, limit));

  const iconFor = (kind: string) => {
    if (kind.startsWith('config')) return FileCogIcon;
    if (kind === 'cert_expiry') return ShieldAlertIcon;
    if (kind === 'circuit_open') return ZapOffIcon;
    if (kind.startsWith('upstream')) return TriangleAlertIcon;
    return CircleAlertIcon;
  };

  /** Relative time, because "2 minutes ago" is what you actually want here. */
  const ago = (epochMs: number): string => {
    const seconds = Math.max(0, Math.round((Date.now() - epochMs) / 1000));
    if (seconds < 10) return 'just now';
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
    return `${Math.round(seconds / 3600)}h ago`;
  };

  // Re-rendered on every tick anyway, so the relative times stay honest without
  // a timer of their own.
</script>

<section class={cn('rounded-xl border border-border bg-card shadow-sm', className)}>
  <header class="border-b border-border px-4 py-3">
    <h3 class="text-sm font-semibold">Recent events</h3>
    <p class="text-xs text-muted-foreground">
      Config applies and reloads, circuits, upstreams and certificates.
    </p>
  </header>

  {#if shown.length === 0}
    <p class="px-4 py-8 text-center text-sm text-muted-foreground">
      Nothing has happened since prx started.
    </p>
  {:else}
    <ul class="divide-y divide-border" data-slot="event-strip">
      {#each shown as event (event.id)}
        {@const Icon = iconFor(event.kind)}
        <li class="flex items-start gap-3 px-4 py-2.5" data-event-kind={event.kind}>
          <Icon
            class={cn(
              'mt-0.5 size-4 shrink-0',
              event.level === 'error' && 'text-destructive-emphasis',
              event.level === 'warn' && 'text-warning-emphasis',
              event.level === 'info' && 'text-muted-foreground'
            )}
            aria-hidden="true"
          />
          <div class="min-w-0 flex-1">
            <p class="text-sm">{event.message}</p>
            <p class="text-xs text-muted-foreground">
              <span class="sr-only">{event.level}. </span>{ago(event.epoch_ms)}
              {#if event.target}
                · {event.target}
              {/if}
            </p>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>
