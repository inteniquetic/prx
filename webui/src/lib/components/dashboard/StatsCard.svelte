<script lang="ts">
  export let icon: string = '';
  export let label: string = '';
  export let value: string | number = 0;
  export let trend: 'up' | 'down' | 'neutral' | undefined = undefined;
  export let color: string = 'slate';

  const colorMap: Record<string, { icon: string; value: string; trend: string; bg: string }> = {
    slate: {
      icon: 'text-muted-foreground',
      value: 'text-foreground',
      trend: 'text-muted-foreground',
      bg: 'from-muted-foreground/5 to-transparent'
    },
    cyan: {
      icon: 'text-primary',
      value: 'text-primary',
      trend: 'text-primary',
      bg: 'from-primary/5 to-transparent'
    },
    emerald: {
      icon: 'text-success',
      value: 'text-success',
      trend: 'text-success',
      bg: 'from-success/5 to-transparent'
    },
    amber: {
      icon: 'text-warning',
      value: 'text-warning',
      trend: 'text-warning',
      bg: 'from-warning/5 to-transparent'
    },
    rose: {
      icon: 'text-destructive',
      value: 'text-destructive',
      trend: 'text-destructive',
      bg: 'from-destructive/5 to-transparent'
    },
    violet: {
      icon: 'text-primary',
      value: 'text-primary',
      trend: 'text-primary',
      bg: 'from-primary/5 to-transparent'
    }
  };

  $: colors = colorMap[color] ?? colorMap.slate;

  $: trendIcon = trend === 'up' ? '↑' : trend === 'down' ? '↓' : '';
  $: trendColorClass = trend === 'up'
    ? 'text-success'
    : trend === 'down'
      ? 'text-destructive'
      : colors.trend;
</script>

<div
  class="group relative overflow-hidden rounded-xl border border-border/80 bg-card/80 p-4 backdrop-blur transition-all duration-200 hover:border-border/80 hover:bg-card/90"
>
  <!-- Subtle gradient background -->
  <div class="pointer-events-none absolute inset-0 bg-gradient-to-br {colors.bg} opacity-0 transition-opacity duration-200 group-hover:opacity-100" ></div>

  <div class="relative flex items-start justify-between gap-3">
    <!-- Icon + Label -->
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2">
        <span class="text-lg leading-none {colors.icon}">{icon}</span>
        <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </span>
      </div>
    </div>

    <!-- Trend indicator -->
    {#if trend && trend !== 'neutral'}
      <span class="flex items-center gap-0.5 text-xs font-semibold {trendColorClass}">
        {trendIcon}
      </span>
    {/if}
  </div>

  <!-- Value -->
  <div class="relative mt-3">
    <span class="text-2xl font-bold tabular-nums tracking-tight {colors.value}">
      {value}
    </span>
  </div>
</div>