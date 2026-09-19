<script lang="ts" module>
  export type UpstreamStatus =
    | 'healthy'
    | 'degraded'
    | 'down'
    | 'circuit-open'
    | 'disabled'
    | 'unknown';

  /**
   * Colour is never the only carrier: each status also has its own glyph and
   * its own word, so the dot still reads on a monochrome screen and to anyone
   * who cannot separate red from green.
   */
  export const STATUS_META: Record<
    UpstreamStatus,
    { label: string; dot: string; icon: string; text: string }
  > = {
    healthy: {
      label: 'Healthy',
      dot: 'bg-success',
      icon: 'text-success-emphasis',
      text: 'text-success-emphasis'
    },
    degraded: {
      label: 'Degraded',
      dot: 'bg-warning',
      icon: 'text-warning-emphasis',
      text: 'text-warning-emphasis'
    },
    down: {
      label: 'Down',
      dot: 'bg-destructive',
      icon: 'text-destructive-emphasis',
      text: 'text-destructive-emphasis'
    },
    'circuit-open': {
      label: 'Circuit open',
      dot: 'bg-circuit',
      icon: 'text-circuit-emphasis',
      text: 'text-circuit-emphasis'
    },
    disabled: {
      label: 'Disabled',
      dot: 'bg-muted-foreground',
      icon: 'text-muted-foreground',
      text: 'text-muted-foreground'
    },
    unknown: {
      label: 'Unknown',
      dot: 'bg-muted-foreground',
      icon: 'text-muted-foreground',
      text: 'text-muted-foreground'
    }
  };
</script>

<script lang="ts">
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import ZapOffIcon from '@lucide/svelte/icons/zap-off';
  import CircleSlashIcon from '@lucide/svelte/icons/circle-slash';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';
  import * as Tooltip from '../tooltip';
  import { cn } from '$lib/utils';

  let {
    status = 'unknown',
    /** Why it is in this state — shown in the tooltip and read out with the status. */
    reason,
    /** Show the status word next to the dot rather than only to screen readers. */
    showLabel = false,
    /**
     * A tooltip costs a floating-layer provider per instance, which adds up in
     * a table of hundreds of rows. `false` keeps the glyph, the word and the
     * reason, and hands the reason to the browser's own tooltip instead.
     */
    tooltip = true,
    class: className
  }: {
    status?: UpstreamStatus;
    reason?: string;
    showLabel?: boolean;
    tooltip?: boolean;
    class?: string;
  } = $props();

  const meta = $derived(STATUS_META[status] ?? STATUS_META.unknown);
  const Icon = $derived(
    status === 'healthy'
      ? CircleCheckIcon
      : status === 'degraded'
        ? TriangleAlertIcon
        : status === 'down'
          ? CircleXIcon
          : status === 'circuit-open'
            ? ZapOffIcon
            : status === 'disabled'
              ? CircleSlashIcon
              : CircleHelpIcon
  );
  const spoken = $derived(reason ? `${meta.label}: ${reason}` : meta.label);
</script>

{#snippet marker()}
  <!-- The glyph differs per status, not just its colour: a check, a warning
       triangle, a cross, a struck-through bolt. -->
  <Icon class={cn('size-4 shrink-0', meta.icon)} aria-hidden="true" />
  {#if showLabel}
    <span aria-hidden="true">{meta.label}</span>
  {/if}
  <span class="sr-only">{spoken}</span>
{/snippet}

{#if tooltip}
  <Tooltip.Root>
    <Tooltip.Trigger
      data-slot="status-dot"
      data-status={status}
      class={cn(
        'inline-flex cursor-default items-center gap-1.5 rounded-sm text-xs font-medium focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none',
        meta.text,
        className
      )}
    >
      {@render marker()}
    </Tooltip.Trigger>
    <Tooltip.Content>
      <span class="flex items-center gap-1.5">
        <Icon class="size-3.5" aria-hidden="true" />
        <span>{spoken}</span>
      </span>
    </Tooltip.Content>
  </Tooltip.Root>
{:else}
  <span
    data-slot="status-dot"
    data-status={status}
    title={spoken}
    class={cn('inline-flex items-center gap-1.5 text-xs font-medium', meta.text, className)}
  >
    {@render marker()}
  </span>
{/if}
