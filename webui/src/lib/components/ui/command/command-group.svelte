<script lang="ts">
  import { Command as CommandPrimitive, type WithoutChild } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    heading,
    children,
    ...restProps
  }: WithoutChild<CommandPrimitive.GroupProps> & { heading?: string; children?: Snippet } =
    $props();
</script>

<CommandPrimitive.Group
  bind:ref
  data-slot="command-group"
  class={cn('overflow-hidden p-1 text-foreground', className)}
  {...restProps}
>
  {#if heading}
    <CommandPrimitive.GroupHeading
      class="px-2 py-1.5 text-xs font-medium text-muted-foreground"
    >
      {heading}
    </CommandPrimitive.GroupHeading>
  {/if}
  <CommandPrimitive.GroupItems>
    {@render children?.()}
  </CommandPrimitive.GroupItems>
</CommandPrimitive.Group>
