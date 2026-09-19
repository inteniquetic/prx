<script lang="ts">
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  // bits-ui has no Select.Separator, so this is a plain divider carrying the
  // right role — it is decoration between groups, not a focusable item.
  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
  bind:this={ref}
  data-slot="select-separator"
  role="separator"
  aria-orientation="horizontal"
  class={cn('-mx-1 my-1 h-px bg-border', className)}
  {...restProps}
></div>
