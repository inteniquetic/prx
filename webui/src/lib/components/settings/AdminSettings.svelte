<script lang="ts">
  /**
   * The admin API's own settings (T308, waiting on T201).
   *
   * There is nothing to edit yet: authentication, allowed origins and
   * read-only mode are not in the config schema — the admin API is protected by
   * binding it to loopback and nothing else. Inventing switches for settings
   * the proxy does not read would be worse than saying so, because a switch
   * labelled "require a token" that does nothing is a security hole with a
   * reassuring label on it.
   */
  import LockIcon from '@lucide/svelte/icons/lock';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { Badge } from '$lib/components/ui/badge';
  import { t } from '$lib/i18n';

  const planned = ['auth', 'origins', 'readonly'];
</script>

<div class="max-w-4xl space-y-4">
  <section
    class="flex items-start gap-3 rounded-2xl border border-warning/40 bg-warning/10 p-6"
    role="note"
  >
    <TriangleAlertIcon class="mt-0.5 size-5 shrink-0 text-warning-emphasis" aria-hidden="true" />
    <div>
      <h3 class="text-sm font-semibold text-foreground">{$t('admin.warning.title')}</h3>
      <p class="mt-1 text-sm text-muted-foreground">{$t('admin.warning.body')}</p>
      <p class="mt-2 text-xs text-muted-foreground">{$t('admin.warning.env')}</p>
    </div>
  </section>

  <section class="rounded-2xl border border-border/80 bg-card/80 p-6">
    <h3 class="flex items-center gap-2 text-sm font-semibold text-foreground">
      <LockIcon class="size-4 text-muted-foreground" aria-hidden="true" />
      {$t('admin.planned')}
      <Badge variant="outline">T201</Badge>
    </h3>
    <ul class="mt-3 space-y-2">
      {#each planned as item (item)}
        <li class="text-sm">
          <span class="text-foreground">{$t(`admin.planned.${item}`)}</span>
          <span class="mx-1 text-border">·</span>
          <span class="text-muted-foreground">{$t(`admin.planned.${item}Detail`)}</span>
        </li>
      {/each}
    </ul>
    <p class="mt-3 text-xs text-muted-foreground">{$t('admin.planned.note')}</p>
  </section>
</div>
