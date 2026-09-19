<script lang="ts">
  import type { Snippet } from 'svelte';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import * as Dialog from '../dialog';
  import { Button } from '../button';

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    /** `destructive` for anything that deletes or interrupts traffic. */
    variant = 'default',
    /** Held open with the button busy while the action runs. */
    pending = false,
    onconfirm,
    oncancel,
    children
  }: {
    open?: boolean;
    title: string;
    description?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    variant?: 'default' | 'destructive';
    pending?: boolean;
    onconfirm?: () => void;
    oncancel?: () => void;
    children?: Snippet;
  } = $props();

  function cancel() {
    open = false;
    oncancel?.();
  }
</script>

<Dialog.Root bind:open onOpenChange={(next) => !next && oncancel?.()}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2">
        {#if variant === 'destructive'}
          <TriangleAlertIcon class="size-4 text-destructive-emphasis" aria-hidden="true" />
        {/if}
        {title}
      </Dialog.Title>
      {#if description}
        <Dialog.Description>{description}</Dialog.Description>
      {/if}
    </Dialog.Header>

    {#if children}
      <div class="text-sm">{@render children()}</div>
    {/if}

    <Dialog.Footer>
      <Button variant="outline" onclick={cancel} disabled={pending}>{cancelLabel}</Button>
      <Button
        variant={variant === 'destructive' ? 'destructive' : 'default'}
        onclick={() => onconfirm?.()}
        disabled={pending}
      >
        {confirmLabel}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
