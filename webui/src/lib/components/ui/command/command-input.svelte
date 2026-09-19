<script lang="ts">
  import { Command as CommandPrimitive } from 'bits-ui';
  import SearchIcon from '@lucide/svelte/icons/search';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    value = $bindable(''),
    class: className,
    ...restProps
  }: CommandPrimitive.InputProps = $props();
</script>

<div data-slot="command-input-wrapper" class="flex h-11 items-center gap-2 border-b border-border px-3">
  <SearchIcon class="size-4 shrink-0 text-muted-foreground" />
  <!-- `role="combobox"` comes from bits-ui, and ARIA requires `aria-expanded`
       and a list to point at with it. The palette's list is always on screen
       while the input is, and `command-list.svelte` gives it this id. -->
  <CommandPrimitive.Input
    bind:ref
    bind:value
    aria-expanded="true"
    aria-controls="prx-command-list"
    data-slot="command-input"
    class={cn(
      'flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50',
      className
    )}
    {...restProps}
  />
</div>
