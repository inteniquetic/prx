<script lang="ts">
  export let icon: string = '';
  export let label: string = '';
  export let value: string | number = 0;
  export let trend: 'up' | 'down' | 'neutral' | undefined = undefined;
  export let color: string = 'slate';

  const colorMap: Record<string, { icon: string; value: string; trend: string; bg: string }> = {
    slate: {
      icon: 'text-slate-400',
      value: 'text-slate-100',
      trend: 'text-slate-500',
      bg: 'from-slate-500/5 to-transparent'
    },
    cyan: {
      icon: 'text-cyan-400',
      value: 'text-cyan-100',
      trend: 'text-cyan-500',
      bg: 'from-cyan-500/5 to-transparent'
    },
    emerald: {
      icon: 'text-emerald-400',
      value: 'text-emerald-100',
      trend: 'text-emerald-500',
      bg: 'from-emerald-500/5 to-transparent'
    },
    amber: {
      icon: 'text-amber-400',
      value: 'text-amber-100',
      trend: 'text-amber-500',
      bg: 'from-amber-500/5 to-transparent'
    },
    rose: {
      icon: 'text-rose-400',
      value: 'text-rose-100',
      trend: 'text-rose-500',
      bg: 'from-rose-500/5 to-transparent'
    },
    violet: {
      icon: 'text-violet-400',
      value: 'text-violet-100',
      trend: 'text-violet-500',
      bg: 'from-violet-500/5 to-transparent'
    }
  };

  $: colors = colorMap[color] ?? colorMap.slate;

  $: trendIcon = trend === 'up' ? '↑' : trend === 'down' ? '↓' : '';
  $: trendColorClass = trend === 'up'
    ? 'text-emerald-400'
    : trend === 'down'
      ? 'text-rose-400'
      : colors.trend;
</script>

<div
  class="group relative overflow-hidden rounded-xl border border-slate-700/80 bg-slate-900/80 p-4 backdrop-blur transition-all duration-200 hover:border-slate-600/80 hover:bg-slate-900/90"
>
  <!-- Subtle gradient background -->
  <div class="pointer-events-none absolute inset-0 bg-gradient-to-br {colors.bg} opacity-0 transition-opacity duration-200 group-hover:opacity-100" />

  <div class="relative flex items-start justify-between gap-3">
    <!-- Icon + Label -->
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2">
        <span class="text-lg leading-none {colors.icon}">{icon}</span>
        <span class="text-xs font-semibold uppercase tracking-wider text-slate-400">
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