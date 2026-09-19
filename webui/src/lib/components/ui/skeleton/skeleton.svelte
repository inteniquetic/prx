<script lang="ts">
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<!-- Purely decorative: a screen reader should hear the loading state from the
     region's aria-busy, not from a row of empty boxes. -->
<div
  bind:this={ref}
  data-slot="skeleton"
  aria-hidden="true"
  class={cn('animate-pulse rounded-md bg-muted', className)}
  {...restProps}
></div>
