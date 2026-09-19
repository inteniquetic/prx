<script lang="ts">
  import ActivityIcon from '@lucide/svelte/icons/activity';
  import FileDiffIcon from '@lucide/svelte/icons/file-diff';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RouteIcon from '@lucide/svelte/icons/route';
  import ServerIcon from '@lucide/svelte/icons/server';
  import UploadIcon from '@lucide/svelte/icons/upload';
  import type { Component } from 'svelte';
  import * as Command from '$lib/components/ui/command';
  import { navigate, navItems, type NavPage } from '$lib/stores/navigation';
  import type { PrxConfig } from '$lib/types/config';
  import { plural, t } from '$lib/i18n';

  let {
    open = $bindable(false),
    config,
    hasDraft = false,
    onaddRoute,
    onapplyDraft,
    onrefreshHealth
  }: {
    open?: boolean;
    config: PrxConfig;
    hasDraft?: boolean;
    onaddRoute?: () => void;
    onapplyDraft?: () => void;
    onrefreshHealth?: () => void;
  } = $props();

  type Entry = {
    id: string;
    label: string;
    hint: string;
    /** Everything the query is matched against, lowercased once up front. */
    haystack: string;
    icon: Component;
    run: () => void;
  };

  let query = $state('');

  // Only as many rows as a person can actually scan. With 500 routes the cost
  // of opening the palette has to be the cost of drawing a dozen lines, not of
  // drawing every route in the config.
  const LIMIT = 7;

  const pageEntries = $derived(
    navItems.map((item) => ({
      id: `page:${item.id}`,
      label: $t(item.labelKey),
      hint: item.pending
        ? $t('palette.hint.placeholder', { task: item.pending })
        : $t('palette.hint.page'),
      haystack: `${$t(item.labelKey)} ${item.id}`.toLowerCase(),
      icon: item.icon,
      run: () => navigate(item.id as NavPage)
    }))
  );

  const routeEntries = $derived(
    (config.routes ?? []).map((route) => ({
      id: `route:${route.name}`,
      label: route.name,
      hint: [route.host || $t('common.anyHost'), route.path_prefix, `→ ${route.service}`]
        .filter(Boolean)
        .join('  '),
      haystack: `${route.name} ${route.host} ${route.path_prefix} ${route.service}`.toLowerCase(),
      icon: RouteIcon,
      run: () => navigate({ page: 'routes', name: route.name })
    }))
  );

  const serviceEntries = $derived(
    (config.services ?? []).map((service) => ({
      id: `service:${service.name}`,
      label: service.name,
      hint: `${$plural('palette.upstreams', service.upstreams.length)}  ${service.upstreams
        .slice(0, 2)
        .map((upstream) => upstream.addr)
        .join(', ')}`,
      haystack: `${service.name} ${service.upstreams.map((upstream) => upstream.addr).join(' ')}`.toLowerCase(),
      icon: ServerIcon,
      run: () => navigate({ page: 'services', name: service.name })
    }))
  );

  const actionEntries = $derived(
    [
      {
        id: 'action:add-route',
        label: $t('palette.action.addRoute'),
        hint: $t('palette.action.addRouteHint'),
        haystack: 'add route new create',
        icon: PlusIcon,
        run: () => onaddRoute?.()
      },
      {
        id: 'action:health',
        label: $t('palette.action.health'),
        hint: $t('palette.action.healthHint'),
        haystack: 'check health probe upstream',
        icon: ActivityIcon,
        run: () => onrefreshHealth?.()
      },
      ...(hasDraft
        ? [
            {
              id: 'action:apply',
              label: $t('palette.action.apply'),
              hint: $t('palette.action.applyHint'),
              haystack: 'apply draft save config',
              icon: UploadIcon,
              run: () => onapplyDraft?.()
            },
            {
              id: 'action:diff',
              label: $t('palette.action.diff'),
              hint: $t('palette.action.diffHint'),
              haystack: 'diff review draft toml settings',
              icon: FileDiffIcon,
              run: () => navigate('settings')
            }
          ]
        : [])
    ] satisfies Entry[]
  );

  /** Prefix beats substring in the name, which beats a hit anywhere else. */
  function score(entry: Entry, needle: string): number {
    if (!needle) return 1;
    const label = entry.label.toLowerCase();
    if (label.startsWith(needle)) return 3;
    if (label.includes(needle)) return 2;
    return entry.haystack.includes(needle) ? 1 : 0;
  }

  function filter(entries: Entry[], needle: string): Entry[] {
    if (!needle) return entries.slice(0, LIMIT);
    const hits: { entry: Entry; score: number }[] = [];
    for (const entry of entries) {
      const value = score(entry, needle);
      if (value > 0) hits.push({ entry, score: value });
    }
    hits.sort((a, b) => b.score - a.score);
    return hits.slice(0, LIMIT).map((hit) => hit.entry);
  }

  const needle = $derived(query.trim().toLowerCase());
  const pages = $derived(filter(pageEntries, needle));
  const routes = $derived(filter(routeEntries, needle));
  const services = $derived(filter(serviceEntries, needle));
  const actions = $derived(filter(actionEntries, needle));
  const empty = $derived(
    pages.length + routes.length + services.length + actions.length === 0
  );

  function run(entry: Entry) {
    open = false;
    query = '';
    entry.run();
  }
</script>

{#snippet group(heading: string, entries: Entry[])}
  {#if entries.length > 0}
    <Command.Group {heading}>
      {#each entries as entry (entry.id)}
        <Command.Item value={entry.id} onSelect={() => run(entry)}>
          <entry.icon aria-hidden="true" />
          <span class="truncate">{entry.label}</span>
          <span class="ml-auto truncate pl-3 text-xs text-muted-foreground">{entry.hint}</span>
        </Command.Item>
      {/each}
    </Command.Group>
  {/if}
{/snippet}

<!-- Filtering is ours, not bits-ui's: matching on host and upstream address as
     well as name, and capping each group, is what keeps a 500-route config
     instant. -->
<Command.Dialog
  bind:open
  shouldFilter={false}
  title={$t('shell.commandPalette')}
  description={$t('palette.placeholder')}
>
  <Command.Input bind:value={query} placeholder={$t('palette.placeholder')} />
  <Command.List>
    {#if empty}
      <Command.Empty>{$t('palette.empty')}</Command.Empty>
    {/if}
    {@render group($t('palette.group.actions'), actions)}
    {@render group($t('palette.group.routes'), routes)}
    {@render group($t('palette.group.services'), services)}
    {@render group($t('palette.group.pages'), pages)}
  </Command.List>
</Command.Dialog>
