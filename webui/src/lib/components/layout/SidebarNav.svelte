<script lang="ts">
  import type { Snippet } from 'svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { Badge } from '$lib/components/ui/badge';
  import {
    currentPage,
    isPlainClick,
    navigate,
    navItems,
    navSections,
    pathFor,
    type NavBadge,
    type NavItem,
    type NavPage
  } from '$lib/stores/navigation';
  import { cn } from '$lib/utils';

  let {
    /** Counts shown next to a menu entry, e.g. upstreams that are down. */
    badges = {},
    collapsed = false,
    onnavigate
  }: {
    badges?: Partial<Record<NavPage, NavBadge>>;
    collapsed?: boolean;
    /** Lets a drawer close itself once a destination is picked. */
    onnavigate?: (page: NavPage) => void;
  } = $props();

  function go(event: MouseEvent, page: NavPage) {
    if (!isPlainClick(event)) return;
    event.preventDefault();
    navigate(page);
    onnavigate?.(page);
  }
</script>

{#snippet link(item: NavItem, badge: NavBadge | undefined, triggerProps: Record<string, unknown>)}
  {@const active = $currentPage === item.id}
  <a
    {...triggerProps}
    href={pathFor(item.id)}
    aria-current={active ? 'page' : undefined}
    onclick={(event: MouseEvent) => go(event, item.id)}
    class={cn(
      'relative flex items-center gap-3 rounded-md border-l-2 py-2 pr-2 text-sm font-medium transition-colors focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none',
      collapsed ? 'justify-center pl-0' : 'pl-2',
      active
        ? 'border-primary bg-accent text-accent-foreground'
        : 'border-transparent text-muted-foreground hover:bg-accent/60 hover:text-accent-foreground'
    )}
  >
    <item.icon class="size-4 shrink-0" aria-hidden="true" />
    <span class={cn('flex-1 truncate', collapsed && 'sr-only')}>{item.label}</span>

    {#if badge && badge.count > 0}
      <Badge
        variant={badge.tone === 'destructive' ? 'destructive' : 'warning'}
        class={cn('tabular-nums', collapsed && 'absolute right-1 top-1 px-1 py-0 text-[10px]')}
      >
        {badge.count}
        <span class="sr-only">{badge.label}</span>
      </Badge>
    {:else if item.pending && !collapsed}
      <!-- Says the page is a placeholder before anyone clicks it. -->
      <Badge variant="outline" class="text-[10px] font-normal">{item.pending}</Badge>
    {/if}
  </a>
{/snippet}

<nav class="flex flex-1 flex-col gap-4 overflow-y-auto overflow-x-hidden px-2 py-4" aria-label="Main">
  {#each navSections as section (section.id)}
    {@const items = navItems.filter((item) => item.section === section.id)}
    {#if items.length > 0}
      <div class="grid gap-1">
        <!-- The heading stays in the tree when collapsed so the grouping is
             still announced; only its pixels go away. -->
        <p
          class={cn(
            'px-2 text-[11px] font-medium uppercase tracking-wider text-muted-foreground',
            collapsed && 'sr-only'
          )}
        >
          {section.label}
        </p>

        {#each items as item (item.id)}
          {@const badge = badges[item.id]}
          {#if collapsed}
            <!-- Collapsed to icons, the tooltip is the only label there is. -->
            <Tooltip.Root>
              <Tooltip.Trigger>
                {#snippet child({ props }: { props: Record<string, unknown> })}
                  {@render link(item, badge, props)}
                {/snippet}
              </Tooltip.Trigger>
              <Tooltip.Content side="right">
                {item.label}{badge && badge.count > 0 ? ` — ${badge.count} ${badge.label}` : ''}
              </Tooltip.Content>
            </Tooltip.Root>
          {:else}
            {@render link(item, badge, {})}
          {/if}
        {/each}
      </div>
    {/if}
  {/each}
</nav>
