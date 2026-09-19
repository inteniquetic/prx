<script lang="ts">
  import { Select as SelectPrimitive, type WithoutChild } from 'bits-ui';
  import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
  import ChevronUpIcon from '@lucide/svelte/icons/chevron-up';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    sideOffset = 4,
    portalProps,
    children,
    ...restProps
  }: WithoutChild<SelectPrimitive.ContentProps> & {
    portalProps?: SelectPrimitive.PortalProps;
  } = $props();
</script>

<SelectPrimitive.Portal {...portalProps}>
  <SelectPrimitive.Content
    bind:ref
    data-slot="select-content"
    {sideOffset}
    class={cn(
      'relative z-50 max-h-(--bits-select-content-available-height) min-w-[8rem] origin-(--bits-select-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border border-border bg-popover text-popover-foreground shadow-md data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95',
      className
    )}
    {...restProps}
  >
    <SelectPrimitive.ScrollUpButton class="flex cursor-default items-center justify-center py-1">
      <ChevronUpIcon class="size-4" />
    </SelectPrimitive.ScrollUpButton>
    <SelectPrimitive.Viewport class="w-full min-w-(--bits-select-anchor-width) scroll-my-1 p-1">
      {@render children?.()}
    </SelectPrimitive.Viewport>
    <SelectPrimitive.ScrollDownButton class="flex cursor-default items-center justify-center py-1">
      <ChevronDownIcon class="size-4" />
    </SelectPrimitive.ScrollDownButton>
  </SelectPrimitive.Content>
</SelectPrimitive.Portal>
