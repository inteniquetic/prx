<script lang="ts">
  import type { Command as CommandPrimitive, Dialog as DialogPrimitive } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import * as Dialog from '../dialog';
  import Command from './command.svelte';
  import { cn } from '$lib/utils';

  let {
    open = $bindable(false),
    ref = $bindable(null),
    value = $bindable(''),
    title = 'Command palette',
    description = 'Search for a command to run',
    class: className,
    children,
    ...restProps
  }: DialogPrimitive.RootProps &
    CommandPrimitive.RootProps & {
      title?: string;
      description?: string;
      children?: Snippet;
    } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="overflow-hidden p-0" showCloseButton={false}>
    <!-- The palette is visually a search box, but assistive tech still needs a
         name and a purpose for the dialog it lives in. -->
    <Dialog.Header class="sr-only">
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    <Command
      bind:ref
      bind:value
      class={cn(
        "[&_[data-slot=command-group]]:px-2 [&_[data-slot=command-input-wrapper]]:h-12 [&_[data-slot=command-item]]:px-2 [&_[data-slot=command-item]]:py-3",
        className
      )}
      {...restProps}
    >
      {@render children?.()}
    </Command>
  </Dialog.Content>
</Dialog.Root>
