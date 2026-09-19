<script lang="ts">
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLThAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLThAttributes, HTMLTableCellElement> = $props();
</script>

<th
  bind:this={ref}
  data-slot="table-head"
  class={cn(
    'h-10 whitespace-nowrap px-3 text-left align-middle text-xs font-medium uppercase tracking-wide text-muted-foreground [&:has([role=checkbox])]:pr-0',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</th>
