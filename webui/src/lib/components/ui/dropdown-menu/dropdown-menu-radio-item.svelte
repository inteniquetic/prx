<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive, type WithoutChild } from 'bits-ui';
  import CircleIcon from '@lucide/svelte/icons/circle';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    value,
    children: childrenProp,
    ...restProps
  }: WithoutChild<DropdownMenuPrimitive.RadioItemProps> & { children?: Snippet } = $props();
</script>

<DropdownMenuPrimitive.RadioItem
  bind:ref
  {value}
  data-slot="dropdown-menu-radio-item"
  class={cn(
    "relative flex cursor-default select-none items-center gap-2 rounded-sm py-1.5 pl-8 pr-2 text-sm outline-hidden data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-accent data-highlighted:text-accent-foreground [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0",
    className
  )}
  {...restProps}
>
  {#snippet children({ checked })}
    <span class="pointer-events-none absolute left-2 flex size-3.5 items-center justify-center">
      {#if checked}
        <CircleIcon class="size-2 fill-current" aria-hidden="true" />
      {/if}
    </span>
    {@render childrenProp?.()}
  {/snippet}
</DropdownMenuPrimitive.RadioItem>
