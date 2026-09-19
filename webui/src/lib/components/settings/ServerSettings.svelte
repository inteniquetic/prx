<script lang="ts">
  /**
   * The listener and process settings (T308).
   *
   * Every field here writes into the same draft as the TOML editor, one key at
   * a time, so the file keeps its comments and the change still goes through
   * the diff before it reaches the proxy.
   */
  import PlusIcon from '@lucide/svelte/icons/plus';
  import TrashIcon from '@lucide/svelte/icons/trash-2';

  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import { draftConfig, editDraft } from '$lib/stores/configDraft';
  import SettingRow from './SettingRow.svelte';
  import { t } from '$lib/i18n';

  const server = $derived($draftConfig?.server ?? null);

  /** Blank means "not set", which in TOML means taking the key out. */
  const numberOrNull = (raw: string): number | null => {
    const trimmed = raw.trim();
    if (trimmed === '') return null;
    const parsed = Number(trimmed);
    return Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : null;
  };

  const set = (path: string, value: unknown) => void editDraft([{ path, value }]);

  let newListen = $state('');

  function addListen() {
    const address = newListen.trim();
    if (!address || !server) return;
    set('server.listen', [...server.listen, address]);
    newListen = '';
  }

  function removeListen(index: number) {
    if (!server) return;
    set(
      'server.listen',
      server.listen.filter((_, position) => position !== index)
    );
  }

  function updateListen(index: number, address: string) {
    if (!server) return;
    const next = server.listen.slice();
    next[index] = address.trim();
    set('server.listen', next.filter(Boolean));
  }
</script>

{#if server}
  <div class="max-w-4xl">
    <p
      class="mb-4 rounded-xl border border-border bg-muted/40 px-4 py-3 text-xs text-muted-foreground"
    >
      {$t('settings.restartNotice')}
    </p>

    <section class="rounded-2xl border border-border/80 bg-card/80 px-6">
      <SettingRow
        label={$t('server.listen.label')}
        description={$t('server.listen.help')}
        path="server.listen"
        restart
      >
        <div class="space-y-2">
          {#each server.listen as address, index (index)}
            <div class="flex gap-2">
              <Input
                value={address}
                aria-label={$t('server.listen.item', { index: index + 1 })}
                onchange={(event) =>
                  updateListen(index, (event.currentTarget as HTMLInputElement).value)}
              />
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={$t('server.listen.remove', { address })}
                disabled={server.listen.length <= 1}
                onclick={() => removeListen(index)}
              >
                <TrashIcon class="size-4" aria-hidden="true" />
              </Button>
            </div>
          {/each}

          <div class="flex gap-2">
            <Input
              bind:value={newListen}
              placeholder="0.0.0.0:8081"
              aria-label={$t('server.listen.new')}
              onkeydown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  addListen();
                }
              }}
            />
            <Button variant="outline" size="sm" onclick={addListen} disabled={!newListen.trim()}>
              <PlusIcon class="size-4" aria-hidden="true" />
              {$t('common.add')}
            </Button>
          </div>
        </div>
      </SettingRow>

      <SettingRow
        label={$t('server.healthPath.label')}
        description={$t('server.healthPath.help')}
        path="server.health_path"
        restart
        id="server-health-path"
      >
        <Input
          id="server-health-path"
          value={server.health_path}
          onchange={(event) =>
            set('server.health_path', (event.currentTarget as HTMLInputElement).value.trim())}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.readyPath.label')}
        description={$t('server.readyPath.help')}
        path="server.ready_path"
        restart
        id="server-ready-path"
      >
        <Input
          id="server-ready-path"
          value={server.ready_path}
          onchange={(event) =>
            set('server.ready_path', (event.currentTarget as HTMLInputElement).value.trim())}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.threads.label')}
        description={$t('server.threads.help')}
        path="server.threads"
        restart
        id="server-threads"
      >
        <Input
          id="server-threads"
          type="number"
          min="1"
          placeholder={$t('server.threads.placeholder')}
          value={server.threads ?? ''}
          onchange={(event) =>
            set('server.threads', numberOrNull((event.currentTarget as HTMLInputElement).value))}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.h2c.label')}
        description={$t('server.h2c.help')}
        path="server.h2c"
        restart
        id="server-h2c"
      >
        <Switch
          id="server-h2c"
          checked={server.h2c}
          onCheckedChange={(checked) => set('server.h2c', checked)}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.grace.label')}
        description={$t('server.grace.help')}
        path="server.grace_period_seconds"
        restart
        id="server-grace"
      >
        <Input
          id="server-grace"
          type="number"
          min="0"
          placeholder={$t('server.pingoraDefault')}
          value={server.grace_period_seconds ?? ''}
          onchange={(event) =>
            set(
              'server.grace_period_seconds',
              numberOrNull((event.currentTarget as HTMLInputElement).value)
            )}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.shutdown.label')}
        description={$t('server.shutdown.help')}
        path="server.graceful_shutdown_timeout_seconds"
        restart
        id="server-shutdown"
      >
        <Input
          id="server-shutdown"
          type="number"
          min="0"
          placeholder={$t('server.pingoraDefault')}
          value={server.graceful_shutdown_timeout_seconds ?? ''}
          onchange={(event) =>
            set(
              'server.graceful_shutdown_timeout_seconds',
              numberOrNull((event.currentTarget as HTMLInputElement).value)
            )}
        />
      </SettingRow>

      <SettingRow
        label={$t('server.debounce.label')}
        description={$t('server.debounce.help')}
        path="server.config_reload_debounce_ms"
        restart
        id="server-debounce"
      >
        <Input
          id="server-debounce"
          type="number"
          min="50"
          value={server.config_reload_debounce_ms}
          onchange={(event) =>
            set(
              'server.config_reload_debounce_ms',
              numberOrNull((event.currentTarget as HTMLInputElement).value) ?? 250
            )}
        />
      </SettingRow>
    </section>
  </div>
{:else}
  <p class="text-sm text-muted-foreground">{$t('settings.needsValidDraft')}</p>
{/if}
