<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    sideOffset = 4,
    children,
    portalProps,
    ...restProps
  }: TooltipPrimitive.ContentProps & { portalProps?: TooltipPrimitive.PortalProps } = $props();
</script>

<TooltipPrimitive.Portal {...portalProps}>
  <TooltipPrimitive.Content
    bind:ref
    data-slot="tooltip-content"
    {sideOffset}
    class={cn(
      'z-50 w-fit origin-(--bits-tooltip-content-transform-origin) text-balance rounded-md bg-tooltip px-3 py-1.5 text-xs text-tooltip-foreground shadow-md data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95',
      className
    )}
    {...restProps}
  >
    {@render children?.()}
    <TooltipPrimitive.Arrow
      class="z-50 size-2.5 rotate-45 rounded-[2px] bg-tooltip"
    />
  </TooltipPrimitive.Content>
</TooltipPrimitive.Portal>
