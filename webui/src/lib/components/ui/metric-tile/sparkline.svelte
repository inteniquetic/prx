<script lang="ts">
  import { cn } from '$lib/utils';

  let {
    values = [],
    class: className
  }: { values?: number[]; class?: string } = $props();

  const WIDTH = 100;
  const HEIGHT = 28;

  // A flat series still needs a line, so a zero range is drawn down the middle
  // rather than dividing by zero.
  const path = $derived.by(() => {
    if (values.length < 2) return '';
    const min = Math.min(...values);
    const max = Math.max(...values);
    const range = max - min;
    const step = WIDTH / (values.length - 1);
    return values
      .map((value, index) => {
        const x = index * step;
        const y = range === 0 ? HEIGHT / 2 : HEIGHT - ((value - min) / range) * HEIGHT;
        return `${index === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(' ');
  });
</script>

{#if path}
  <!-- Decoration: the numbers above it are the accessible version of this. -->
  <svg
    data-slot="sparkline"
    class={cn('h-7 w-full text-muted-foreground', className)}
    viewBox="0 0 {WIDTH} {HEIGHT}"
    preserveAspectRatio="none"
    aria-hidden="true"
    focusable="false"
  >
    <path d={path} fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
{/if}
