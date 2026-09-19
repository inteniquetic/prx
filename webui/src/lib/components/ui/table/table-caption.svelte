<script lang="ts">
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>, HTMLTableCaptionElement> = $props();
</script>

<caption
  bind:this={ref}
  data-slot="table-caption"
  class={cn('mt-4 text-sm text-muted-foreground', className)}
  {...restProps}
>
  {@render children?.()}
</caption>
