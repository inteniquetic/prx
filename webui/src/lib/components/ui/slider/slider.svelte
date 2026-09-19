<script lang="ts">
  import { Slider as SliderPrimitive, type WithoutChildrenOrChild } from 'bits-ui';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    value = $bindable(),
    orientation = 'horizontal',
    class: className,
    ...restProps
  }: WithoutChildrenOrChild<SliderPrimitive.RootProps> = $props();
</script>

<!-- bits-ui renders one thumb per value, so a single-value slider is `type="single"`. -->
<SliderPrimitive.Root
  bind:ref
  bind:value={value as never}
  {orientation}
  data-slot="slider"
  class={cn(
    'relative flex w-full touch-none select-none items-center data-[disabled]:opacity-50 data-[orientation=vertical]:h-full data-[orientation=vertical]:w-auto data-[orientation=vertical]:flex-col',
    className
  )}
  {...restProps}
>
  {#snippet children({ thumbs })}
    <span
      data-slot="slider-track"
      class="relative grow overflow-hidden rounded-full bg-switch-track data-[orientation=horizontal]:h-1.5 data-[orientation=horizontal]:w-full data-[orientation=vertical]:h-full data-[orientation=vertical]:w-1.5"
    >
      <SliderPrimitive.Range
        data-slot="slider-range"
        class="absolute bg-primary data-[orientation=horizontal]:h-full data-[orientation=vertical]:w-full"
      />
    </span>
    {#each thumbs as index (index)}
      <SliderPrimitive.Thumb
        {index}
        data-slot="slider-thumb"
        class="block size-4 shrink-0 rounded-full border border-primary bg-background shadow-sm transition-[color,box-shadow] hover:ring-4 hover:ring-ring/50 focus-visible:ring-4 focus-visible:ring-ring/50 focus-visible:outline-hidden disabled:pointer-events-none disabled:opacity-50"
      />
    {/each}
  {/snippet}
</SliderPrimitive.Root>
