<script lang="ts">
  import PanelLeftCloseIcon from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpenIcon from '@lucide/svelte/icons/panel-left-open';
  import { Button } from '$lib/components/ui/button';
  import SidebarNav from './SidebarNav.svelte';
  import {
    isPlainClick,
    navigate,
    pathFor,
    sidebarCollapsed,
    toggleSidebar,
    type NavBadge,
    type NavPage
  } from '$lib/stores/navigation';
  import { cn } from '$lib/utils';
  import { t } from '$lib/i18n';

  let { badges = {} }: { badges?: Partial<Record<NavPage, NavBadge>> } = $props();

  const collapsed = $derived($sidebarCollapsed);
</script>

<aside
  data-slot="sidebar"
  class={cn(
    'hidden shrink-0 select-none flex-col border-r border-border bg-sidebar text-sidebar-foreground transition-[width] duration-200 md:flex',
    collapsed ? 'w-16' : 'w-60'
  )}
>
  <div class={cn('flex h-14 items-center border-b border-sidebar-border px-3', collapsed && 'justify-center px-0')}>
    <a
      href={pathFor('dashboard')}
      onclick={(event: MouseEvent) => {
        if (!isPlainClick(event)) return;
        event.preventDefault();
        navigate('dashboard');
      }}
      class="flex items-center gap-2.5 rounded-md focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
    >
      <span
        class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground"
        aria-hidden="true"
      >
        P
      </span>
      <span class={cn('flex flex-col leading-tight', collapsed && 'sr-only')}>
        <span class="text-base font-semibold tracking-tight">PRX</span>
        <span class="text-[10px] uppercase tracking-widest text-muted-foreground">
          {$t('shell.brandSubtitle')}
        </span>
      </span>
    </a>
  </div>

  <SidebarNav {badges} {collapsed} />

  <div class={cn('grid gap-2 border-t border-sidebar-border p-2', collapsed && 'justify-items-center')}>
    <Button
      variant="ghost"
      size={collapsed ? 'icon-sm' : 'sm'}
      class={collapsed ? '' : 'justify-start'}
      onclick={toggleSidebar}
      aria-label={collapsed ? $t('shell.expandSidebar') : $t('shell.collapseSidebar')}
      aria-expanded={!collapsed}
    >
      {#if collapsed}
        <PanelLeftOpenIcon aria-hidden="true" />
      {:else}
        <PanelLeftCloseIcon aria-hidden="true" />
        <span>{$t('shell.collapse')}</span>
      {/if}
    </Button>
  </div>
</aside>
