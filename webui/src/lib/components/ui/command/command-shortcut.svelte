<script lang="ts">
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLSpanElement>, HTMLSpanElement> = $props();
</script>

<span
  bind:this={ref}
  data-slot="command-shortcut"
  class={cn('ml-auto font-mono text-xs tracking-widest text-muted-foreground', className)}
  {...restProps}
>
  {@render children?.()}
</span>
