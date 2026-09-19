<script lang="ts">
  import { Command as CommandPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: CommandPrimitive.ListProps = $props();
</script>

<!-- The arrow keys move the selection from the input, but the list itself
     scrolls, and a scrollable region has to be reachable by keyboard for anyone
     driving the page without a mouse — so it is a tab stop of its own.
     The id is fixed rather than generated so the input can point `aria-controls`
     at it. -->
<CommandPrimitive.List
  bind:ref
  id="prx-command-list"
  tabindex={0}
  data-slot="command-list"
  class={cn('max-h-80 scroll-py-1 overflow-x-hidden overflow-y-auto', className)}
  {...restProps}
/>
