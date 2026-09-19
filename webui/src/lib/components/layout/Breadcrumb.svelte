<script lang="ts">
  import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
  import { isPlainClick, location, navItems, navigate, pathFor } from '$lib/stores/navigation';
  import { t } from '$lib/i18n';

  const item = $derived(navItems.find((entry) => entry.id === $location.page));
  const name = $derived($location.name);
</script>

<nav aria-label={$t('nav.breadcrumb')} class="min-w-0">
  <ol class="flex min-w-0 items-center gap-1.5 text-sm">
    <li class="shrink-0">
      <a
        href={pathFor('dashboard')}
        class="rounded-sm text-muted-foreground hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
        onclick={(event: MouseEvent) => {
          if (!isPlainClick(event)) return;
          event.preventDefault();
          navigate('dashboard');
        }}
      >
        prx
      </a>
    </li>

    {#if item && item.id !== 'dashboard'}
      <li aria-hidden="true" class="shrink-0 text-muted-foreground">
        <ChevronRightIcon class="size-3.5" />
      </li>
      <li class="min-w-0 shrink">
        {#if name}
          <a
            href={pathFor(item.id)}
            class="rounded-sm text-muted-foreground hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
            onclick={(event: MouseEvent) => {
              if (!isPlainClick(event)) return;
              event.preventDefault();
              navigate(item.id);
            }}
          >
            {$t(item.labelKey)}
          </a>
        {:else}
          <span aria-current="page" class="font-medium">{$t(item.labelKey)}</span>
        {/if}
      </li>
    {/if}

    {#if item && name}
      <li aria-hidden="true" class="shrink-0 text-muted-foreground">
        <ChevronRightIcon class="size-3.5" />
      </li>
      <li class="min-w-0">
        <span aria-current="page" class="block truncate font-medium">{name}</span>
      </li>
    {/if}
  </ol>
</nav>
