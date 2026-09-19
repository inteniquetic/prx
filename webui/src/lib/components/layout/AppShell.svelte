<script lang="ts">
  import type { Snippet } from 'svelte';
  import * as Sheet from '$lib/components/ui/sheet';
  import { Toaster } from '$lib/components/ui/sonner';
  import CommandPalette from './CommandPalette.svelte';
  import Sidebar from './Sidebar.svelte';
  import SidebarNav from './SidebarNav.svelte';
  import Topbar from './Topbar.svelte';
  import { type NavBadge, type NavPage } from '$lib/stores/navigation';
  import type { PrxConfig } from '$lib/types/config';

  let {
    config,
    badges = {},
    hasDraft = false,
    onaddRoute,
    onapplyDraft,
    onrefreshHealth,
    children
  }: {
    config: PrxConfig;
    badges?: Partial<Record<NavPage, NavBadge>>;
    hasDraft?: boolean;
    onaddRoute?: () => void;
    onapplyDraft?: () => void;
    onrefreshHealth?: () => void;
    children: Snippet;
  } = $props();

  let navOpen = $state(false);
  let paletteOpen = $state(false);

  function onKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      paletteOpen = !paletteOpen;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Toaster />

<div class="flex h-screen w-full overflow-hidden bg-background text-foreground">
  <!-- First stop for a keyboard user: past the whole sidebar in one keystroke. -->
  <a
    href="#main-content"
    class="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded-md focus:bg-popover focus:px-3 focus:py-2 focus:text-sm focus:shadow-md focus:outline-none focus:ring-[3px] focus:ring-ring/50"
  >
    Skip to content
  </a>

  <Sidebar {badges} />

  <!-- Under md the rail is gone; the same nav comes back as a drawer. -->
  <Sheet.Root bind:open={navOpen}>
    <Sheet.Content side="left" class="w-64 p-0">
      <Sheet.Header class="border-b border-border">
        <Sheet.Title>PRX</Sheet.Title>
        <Sheet.Description class="sr-only">Main navigation</Sheet.Description>
      </Sheet.Header>
      <SidebarNav {badges} onnavigate={() => (navOpen = false)} />
    </Sheet.Content>
  </Sheet.Root>

  <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
    <Topbar
      {hasDraft}
      onopenNav={() => (navOpen = true)}
      onopenPalette={() => (paletteOpen = true)}
    />

    <main id="main-content" tabindex="-1" class="flex min-w-0 flex-1 flex-col overflow-hidden focus:outline-none">
      {@render children()}
    </main>
  </div>
</div>

<CommandPalette
  bind:open={paletteOpen}
  {config}
  {hasDraft}
  {onaddRoute}
  {onapplyDraft}
  {onrefreshHealth}
/>
