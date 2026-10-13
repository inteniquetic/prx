<script lang="ts">
  /**
   * Who changed what, and the versions to go back to (T206, T202).
   *
   * Both lists come from the admin API as it is: audit entries name the config
   * by fingerprint, and a fingerprint is also what a history version carries,
   * so the two can be lined up without any server-side join.
   */
  import { onMount } from 'svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { EmptyState } from '$lib/components/ui/empty-state';
  import { toast } from '$lib/components/ui/sonner';
  import DiffView from '../config/DiffView.svelte';
  import AppLayout from '../layout/AppLayout.svelte';
  import {
    loadAudit,
    loadHistory,
    loadVersionText,
    rollbackTo,
    type AuditRecord,
    type ConfigVersion
  } from '$lib/api/history';
  import { diffText, type DiffOp } from '$lib/configDiff';
  import { formatBytes } from '$lib/format';
  import { formatDateTime, t } from '$lib/i18n';

  let { onapplied }: { onapplied?: () => void } = $props();

  let audit = $state<AuditRecord[]>([]);
  let versions = $state<ConfigVersion[]>([]);
  let loading = $state(true);
  let failed = $state('');
  let diffFor = $state('');
  let diffOps = $state<DiffOp[]>([]);
  let rollbackId = $state('');
  let busy = $state(false);

  async function refresh() {
    loading = true;
    failed = '';
    try {
      [audit, versions] = await Promise.all([loadAudit(), loadHistory()]);
      audit = audit.slice().reverse(); // newest first, like the versions
    } catch (err) {
      failed = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  // The newest version is what the proxy is running.
  async function showDiff(id: string) {
    if (diffFor === id) {
      diffFor = '';
      return;
    }
    try {
      const [old, current] = await Promise.all([loadVersionText(id), loadVersionText(versions[0].id)]);
      diffOps = diffText(old, current);
      diffFor = id;
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  }

  async function confirmRollback() {
    busy = true;
    try {
      await rollbackTo(rollbackId);
      toast.success($t('audit.toast.rolledBack'));
      rollbackId = '';
      diffFor = '';
      onapplied?.();
      await refresh();
    } catch (err) {
      toast.error($t('audit.toast.rollbackFailed', { reason: err instanceof Error ? err.message : String(err) }));
    } finally {
      busy = false;
    }
  }

  const changed = (e: AuditRecord) => e.before !== e.after;
  const versionOf = (fingerprint: string) => versions.find((v) => v.fingerprint === fingerprint);

  onMount(() => void refresh());
</script>

<AppLayout title={$t('audit.title')} subtitle={$t('audit.subtitle')}>
  <svelte:fragment slot="header-actions">
    <Button variant="outline" size="sm" onclick={refresh} disabled={loading}>
      {$t('common.refresh')}
    </Button>
  </svelte:fragment>

  <div class="space-y-8 p-6">
    {#if failed}
      <EmptyState title={$t('audit.loadFailed')} description={failed} />
    {:else}
      <section aria-labelledby="history-heading">
        <h2 id="history-heading" class="mb-3 text-base font-semibold">{$t('audit.history')}</h2>
        {#if !loading && versions.length === 0}
          <p class="text-sm text-muted-foreground">{$t('audit.history.empty')}</p>
        {:else}
          <ul class="divide-y divide-border rounded-lg border border-border">
            {#each versions as version, index (version.id)}
              <li class="p-3">
                <div class="flex flex-wrap items-center gap-3">
                  <span class="text-sm">{formatDateTime(version.ts_ms)}</span>
                  <span class="font-mono text-xs text-muted-foreground">{version.fingerprint}</span>
                  <span class="text-xs text-muted-foreground">{formatBytes(version.bytes)}</span>
                  {#if index === 0}<Badge variant="outline">{$t('audit.running')}</Badge>{/if}
                  {#if index > 0}
                    <span class="ml-auto flex gap-2">
                      <Button variant="outline" size="sm" onclick={() => showDiff(version.id)}>
                        {diffFor === version.id ? $t('audit.hideDiff') : $t('audit.diff')}
                      </Button>
                      <Button size="sm" onclick={() => (rollbackId = version.id)}>
                        {$t('audit.rollback')}
                      </Button>
                    </span>
                  {/if}
                </div>
                {#if diffFor === version.id}
                  <DiffView
                    class="mt-3"
                    ops={diffOps}
                    beforeLabel={$t('audit.thisVersion')}
                    afterLabel={$t('diff.beforeLabel')}
                  />
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section aria-labelledby="audit-heading">
        <h2 id="audit-heading" class="mb-3 text-base font-semibold">{$t('audit.log')}</h2>
        {#if !loading && audit.length === 0}
          <p class="text-sm text-muted-foreground">{$t('audit.log.empty')}</p>
        {:else}
          <div class="overflow-x-auto rounded-lg border border-border">
            <table class="w-full text-left text-sm">
              <thead class="bg-muted/50 text-xs text-muted-foreground">
                <tr>
                  <th class="px-3 py-2 font-medium">{$t('audit.col.time')}</th>
                  <th class="px-3 py-2 font-medium">{$t('audit.col.actor')}</th>
                  <th class="px-3 py-2 font-medium">{$t('audit.col.action')}</th>
                  <th class="px-3 py-2 font-medium">{$t('audit.col.status')}</th>
                  <th class="px-3 py-2 font-medium">{$t('audit.col.config')}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                {#each audit as entry (`${entry.ts_ms}-${entry.path}-${entry.after}`)}
                  <tr>
                    <td class="whitespace-nowrap px-3 py-2">{formatDateTime(entry.ts_ms)}</td>
                    <td class="px-3 py-2">{entry.actor}</td>
                    <td class="px-3 py-2 font-mono text-xs">{entry.method} {entry.path}</td>
                    <td class="px-3 py-2">
                      <Badge variant={entry.status < 400 ? 'outline' : 'destructive'}>{entry.status}</Badge>
                    </td>
                    <td class="px-3 py-2 text-xs">
                      {#if changed(entry)}
                        <span class="font-mono text-muted-foreground">
                          {$t('audit.changed')}{versionOf(entry.after) ? '' : ` · ${$t('audit.pruned')}`}
                        </span>
                      {:else}
                        <span class="text-muted-foreground">{$t('audit.unchanged')}</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    {/if}
  </div>
</AppLayout>

<ConfirmDialog
  open={rollbackId !== ''}
  variant="destructive"
  title={$t('audit.confirm.title')}
  description={$t('audit.confirm.body')}
  confirmLabel={$t('audit.rollback')}
  pending={busy}
  onconfirm={confirmRollback}
  oncancel={() => (rollbackId = '')}
/>
