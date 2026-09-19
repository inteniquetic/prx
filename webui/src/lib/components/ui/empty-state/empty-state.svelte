<script lang="ts">
  import type { Component, Snippet } from 'svelte';
  import InboxIcon from '@lucide/svelte/icons/inbox';
  import { cn } from '$lib/utils';

  let {
    icon,
    title,
    description,
    action,
    class: className
  }: {
    icon?: Component;
    title: string;
    description?: string;
    /** Usually one Button — the way out of the empty state. */
    action?: Snippet;
    class?: string;
  } = $props();

  const Icon = $derived(icon ?? InboxIcon);
</script>

<div
  data-slot="empty-state"
  class={cn(
    'flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border px-6 py-12 text-center',
    className
  )}
>
  <span class="flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
    <Icon class="size-5" aria-hidden="true" />
  </span>
  <div class="grid gap-1">
    <p class="text-sm font-medium">{title}</p>
    {#if description}
      <p class="max-w-sm text-sm text-muted-foreground">{description}</p>
    {/if}
  </div>
  {#if action}
    <div class="mt-1">{@render action()}</div>
  {/if}
</div>
