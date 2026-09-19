<script lang="ts">
  import FileClockIcon from '@lucide/svelte/icons/file-clock';
  import MenuIcon from '@lucide/svelte/icons/menu';
  import MonitorIcon from '@lucide/svelte/icons/monitor';
  import MoonIcon from '@lucide/svelte/icons/moon';
  import SearchIcon from '@lucide/svelte/icons/search';
  import SunIcon from '@lucide/svelte/icons/sun';
  import UserIcon from '@lucide/svelte/icons/user';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Button } from '$lib/components/ui/button';
  import Breadcrumb from './Breadcrumb.svelte';
  import ConnectionBadge from './ConnectionBadge.svelte';
  import { navigate } from '$lib/stores/navigation';
  import { setTheme, themeChoice, type ThemeChoice } from '$lib/stores/theme';

  let {
    /** True while the editor holds changes that have not been applied. */
    hasDraft = false,
    onopenPalette,
    onopenNav
  }: {
    hasDraft?: boolean;
    onopenPalette?: () => void;
    onopenNav?: () => void;
  } = $props();

  const THEMES: { id: ThemeChoice; label: string; icon: typeof SunIcon }[] = [
    { id: 'light', label: 'Light', icon: SunIcon },
    { id: 'dark', label: 'Dark', icon: MoonIcon },
    { id: 'system', label: 'System', icon: MonitorIcon }
  ];

  const theme = $derived(THEMES.find((entry) => entry.id === $themeChoice) ?? THEMES[2]);
</script>

<header
  data-slot="topbar"
  class="flex h-14 shrink-0 items-center gap-2 border-b border-border bg-background px-3 sm:px-4"
>
  <Button
    variant="ghost"
    size="icon-sm"
    class="md:hidden"
    onclick={() => onopenNav?.()}
    aria-label="Open navigation"
  >
    <MenuIcon aria-hidden="true" />
  </Button>

  <Breadcrumb />

  <div class="ml-auto flex shrink-0 items-center gap-1 sm:gap-2">
    <!-- On a phone this collapses to the icon; the shortcut hint would be a lie
         there anyway. -->
    <Button
      variant="outline"
      size="sm"
      class="gap-2 text-muted-foreground"
      onclick={() => onopenPalette?.()}
      aria-keyshortcuts="Meta+K Control+K"
    >
      <SearchIcon aria-hidden="true" />
      <span class="hidden sm:inline">Search</span>
      <kbd class="hidden rounded border border-border px-1 font-mono text-[10px] sm:inline">⌘K</kbd>
      <span class="sr-only">Open the command palette</span>
    </Button>

    {#if hasDraft}
      <Button
        variant="ghost"
        size="sm"
        class="gap-1.5 text-warning-emphasis"
        onclick={() => navigate('settings')}
      >
        <FileClockIcon aria-hidden="true" />
        <span class="hidden sm:inline">Draft not applied</span>
        <span class="sr-only">Unapplied changes — go to Settings to review and apply</span>
      </Button>
    {/if}

    <div class="hidden md:block">
      <ConnectionBadge />
    </div>

    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props }: { props: Record<string, unknown> })}
          <Button {...props} variant="ghost" size="icon-sm" aria-label={`Theme: ${theme.label}`}>
            <theme.icon aria-hidden="true" />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="w-40">
        <!-- Radio items, not plain ones: it is one choice out of three, and the
             menu should say which is current without a tick glued to the label.
             The heading lives inside the group because bits-ui takes it from
             there — outside one it throws on open. -->
        <DropdownMenu.RadioGroup value={$themeChoice} onValueChange={(value) => setTheme(value as ThemeChoice)}>
          <DropdownMenu.Label>Theme</DropdownMenu.Label>
          {#each THEMES as entry (entry.id)}
            <DropdownMenu.RadioItem value={entry.id}>
              <entry.icon aria-hidden="true" />
              <span>{entry.label}</span>
            </DropdownMenu.RadioItem>
          {/each}
        </DropdownMenu.RadioGroup>
      </DropdownMenu.Content>
    </DropdownMenu.Root>

    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props }: { props: Record<string, unknown> })}
          <Button {...props} variant="ghost" size="icon-sm" aria-label="Account">
            <UserIcon aria-hidden="true" />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="w-64">
        <DropdownMenu.Group>
          <DropdownMenu.Label>Account</DropdownMenu.Label>
          <!-- The admin API takes no credentials yet, so there is no session to
               end. Saying that is more useful than a Sign out that does nothing. -->
          <DropdownMenu.Item disabled>Sign out — no auth yet (T201)</DropdownMenu.Item>
        </DropdownMenu.Group>
        <DropdownMenu.Separator />
        <DropdownMenu.Group>
          <DropdownMenu.Label>Shortcuts</DropdownMenu.Label>
          <DropdownMenu.Item onSelect={() => onopenPalette?.()}>
            Command palette
            <DropdownMenu.Shortcut>⌘K</DropdownMenu.Shortcut>
          </DropdownMenu.Item>
        </DropdownMenu.Group>
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </div>
</header>
