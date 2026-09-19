<script lang="ts">
  /**
   * Certificates: what is being served, and what the file asks for (T308).
   *
   * Two different things, and the page keeps them apart. The top section is the
   * running proxy — the certificates in memory and when they expire, which is
   * the question someone opens this page with. Everything below it edits the
   * draft, and changes nothing until it is applied and prx is restarted.
   */
  import { onMount } from 'svelte';

  import KeyRoundIcon from '@lucide/svelte/icons/key-round';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
  import TrashIcon from '@lucide/svelte/icons/trash-2';

  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { toast } from '$lib/components/ui/sonner';
  import { loadTlsStatus, requestAcmeRenew, type TlsStatus } from '$lib/api/configText';
  import { draftConfig, editDraft } from '$lib/stores/configDraft';
  import SettingRow from './SettingRow.svelte';
  import { formatDate, formatDateTime, locale, plural, t } from '$lib/i18n';

  const tls = $derived($draftConfig?.server.tls ?? null);
  const acme = $derived(tls?.acme ?? null);

  let status: TlsStatus | null = $state(null);
  let statusError = $state('');
  let loadingStatus = $state(false);
  let renewing = $state(false);
  let confirmDisableTls = $state(false);

  const LETS_ENCRYPT = 'https://acme-v02.api.letsencrypt.org/directory';
  const LETS_ENCRYPT_STAGING = 'https://acme-staging-v02.api.letsencrypt.org/directory';

  const set = (path: string, value: unknown) => void editDraft([{ path, value }]);

  const list = (raw: string): string[] =>
    raw
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);

  async function refreshStatus() {
    loadingStatus = true;
    statusError = '';
    try {
      status = await loadTlsStatus();
    } catch (error) {
      statusError = error instanceof Error ? error.message : String(error);
    } finally {
      loadingStatus = false;
    }
  }

  onMount(() => {
    void refreshStatus();
  });

  function enableTls(enabled: boolean) {
    if (!enabled) {
      confirmDisableTls = true;
      return;
    }
    // The smallest table prx accepts: a listener and somewhere to read a
    // certificate from. Both are editable right below.
    void editDraft([
      {
        path: 'server.tls',
        value: {
          listen: '0.0.0.0:8443',
          cert_path: './certs/tls.crt',
          key_path: './certs/tls.key',
          enable_h2: true
        }
      }
    ]);
  }

  function addCert() {
    void editDraft([
      {
        path: 'server.tls.cert',
        action: 'append',
        value: { domains: [], cert_path: '', key_path: '' }
      }
    ]);
  }

  async function renewNow() {
    renewing = true;
    try {
      await requestAcmeRenew();
      toast.success($t('tls.acme.ordered'));
      // The order takes a few seconds at best; one refresh shortly after tends
      // to catch the attempt, and the button can always be pressed again.
      window.setTimeout(() => void refreshStatus(), 3000);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      renewing = false;
    }
  }

  const expiryTone = (days: number): 'destructive' | 'warning' | 'success' =>
    days < 0 ? 'destructive' : days < 30 ? 'warning' : 'success';

  const expiryLabel = (days: number): string => {
    if (days < 0) return $plural('tls.expiry.expired', Math.abs(days));
    if (days === 0) return $t('tls.expiry.today');
    return $plural('tls.expiry.left', days);
  };

  const whenever = (epochSeconds: number | null): string =>
    epochSeconds ? formatDateTime(epochSeconds * 1000, $locale) : $t('common.never');
</script>

<div class="max-w-4xl space-y-6">
  <!-- What the proxy is serving right now -->
  <section class="rounded-2xl border border-border/80 bg-card/80 p-6" data-slot="tls-status">
    <div class="mb-4 flex items-start justify-between gap-4">
      <div>
        <h2 class="flex items-center gap-2 text-sm font-semibold text-foreground">
          <ShieldCheckIcon class="size-4 text-primary" aria-hidden="true" />
          {$t('tls.status.title')}
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">{$t('tls.status.help')}</p>
      </div>
      <Button variant="outline" size="sm" onclick={refreshStatus} disabled={loadingStatus}>
        <RefreshCwIcon class="size-4" aria-hidden="true" />
        {loadingStatus ? $t('common.checking') : $t('common.refresh')}
      </Button>
    </div>

    {#if statusError}
      <p class="text-sm text-destructive-emphasis">{statusError}</p>
    {:else if status && status.certificates.length > 0}
      <ul class="divide-y divide-border/60">
        {#each status.certificates as cert (cert.domain)}
          <li class="flex items-center justify-between gap-4 py-2">
            <span class="font-mono text-sm text-foreground">{cert.domain}</span>
            <span class="flex items-center gap-3">
              <span class="text-xs text-muted-foreground">
                {$t('tls.status.until', {
                  date: formatDate(cert.expires_epoch_s * 1000, $locale)
                })}
              </span>
              <Badge variant={expiryTone(cert.expires_in_days)}>
                {expiryLabel(cert.expires_in_days)}
              </Badge>
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="text-sm text-muted-foreground">{$t('tls.status.none')}</p>
    {/if}
  </section>

  {#if tls === null}
    <section class="rounded-2xl border border-border/80 bg-card/80 px-6">
      <SettingRow
        label={$t('tls.listener.label')}
        description={$t('tls.listener.helpOff')}
        path="server.tls"
        restart
        id="tls-enabled"
      >
        <Switch id="tls-enabled" checked={false} onCheckedChange={enableTls} />
      </SettingRow>
    </section>
  {:else}
    <section class="rounded-2xl border border-border/80 bg-card/80 px-6">
      <SettingRow
        label={$t('tls.listener.label')}
        description={$t('tls.listener.helpOn')}
        path="server.tls"
        restart
        id="tls-enabled"
      >
        <Switch id="tls-enabled" checked={true} onCheckedChange={enableTls} />
      </SettingRow>

      <SettingRow
        label={$t('tls.listen.label')}
        path="server.tls.listen"
        restart
        id="tls-listen"
      >
        <Input
          id="tls-listen"
          value={tls.listen}
          onchange={(event) =>
            set('server.tls.listen', (event.currentTarget as HTMLInputElement).value.trim())}
        />
      </SettingRow>

      <SettingRow
        label={$t('tls.h2.label')}
        description={$t('tls.h2.help')}
        path="server.tls.enable_h2"
        restart
        id="tls-h2"
      >
        <Switch
          id="tls-h2"
          checked={tls.enable_h2}
          onCheckedChange={(checked) => set('server.tls.enable_h2', checked)}
        />
      </SettingRow>

      <SettingRow
        label={$t('tls.cert.label')}
        description={$t('tls.cert.help')}
        path="server.tls.cert_path"
        restart
        id="tls-cert-path"
      >
        <div class="space-y-2">
          <Input
            id="tls-cert-path"
            placeholder="./certs/tls.crt"
            value={tls.cert_path}
            onchange={(event) => {
              const next = (event.currentTarget as HTMLInputElement).value.trim();
              set('server.tls.cert_path', next === '' ? null : next);
            }}
          />
          <Input
            placeholder="./certs/tls.key"
            aria-label={$t('tls.cert.keyAria')}
            value={tls.key_path}
            onchange={(event) => {
              const next = (event.currentTarget as HTMLInputElement).value.trim();
              set('server.tls.key_path', next === '' ? null : next);
            }}
          />
        </div>
      </SettingRow>
    </section>

    <!-- SNI certificates -->
    <section class="rounded-2xl border border-border/80 bg-card/80 p-6">
      <div class="mb-4 flex items-start justify-between gap-4">
        <div>
          <h2 class="flex items-center gap-2 text-sm font-semibold text-foreground">
            <KeyRoundIcon class="size-4 text-primary" aria-hidden="true" />
            {$t('tls.sni.title')}
          </h2>
          <p class="mt-1 text-xs text-muted-foreground">{$t('tls.sni.help')}</p>
        </div>
        <Button variant="outline" size="sm" onclick={addCert}>
          <PlusIcon class="size-4" aria-hidden="true" />
          {$t('common.add')}
        </Button>
      </div>

      {#if tls.certs.length === 0}
        <p class="text-sm text-muted-foreground">{$t('tls.sni.none')}</p>
      {:else}
        <ul class="space-y-3">
          {#each tls.certs as cert, index (index)}
            <li class="rounded-xl border border-border/70 p-4" data-slot="sni-cert">
              <div class="mb-3 flex items-center justify-between gap-2">
                <span class="text-xs font-semibold text-muted-foreground">
                  server.tls.cert[{index}]
                </span>
                <div class="flex items-center gap-3">
                  <label class="flex items-center gap-2 text-xs text-muted-foreground">
                    <Switch
                      checked={cert.is_default}
                      onCheckedChange={(checked) =>
                        set(`server.tls.cert[${index}].is_default`, checked || null)}
                    />
                    {$t('common.default')}
                  </label>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={$t('tls.sni.remove', { index: index + 1 })}
                    onclick={() =>
                      void editDraft([
                        { path: `server.tls.cert[${index}]`, action: 'remove' }
                      ])}
                  >
                    <TrashIcon class="size-4" aria-hidden="true" />
                  </Button>
                </div>
              </div>

              <div class="grid gap-2 sm:grid-cols-3">
                <Input
                  placeholder="example.com, *.example.com"
                  aria-label={$t('tls.sni.domainsAria', { index: index + 1 })}
                  value={cert.domains.join(', ')}
                  onchange={(event) =>
                    set(
                      `server.tls.cert[${index}].domains`,
                      list((event.currentTarget as HTMLInputElement).value)
                    )}
                />
                <Input
                  placeholder="/etc/prx/example.crt"
                  aria-label={$t('tls.sni.certAria', { index: index + 1 })}
                  value={cert.cert_path}
                  onchange={(event) =>
                    set(
                      `server.tls.cert[${index}].cert_path`,
                      (event.currentTarget as HTMLInputElement).value.trim()
                    )}
                />
                <Input
                  placeholder="/etc/prx/example.key"
                  aria-label={$t('tls.sni.keyAria', { index: index + 1 })}
                  value={cert.key_path}
                  onchange={(event) =>
                    set(
                      `server.tls.cert[${index}].key_path`,
                      (event.currentTarget as HTMLInputElement).value.trim()
                    )}
                />
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- ACME -->
    {#if acme}
      <section class="rounded-2xl border border-border/80 bg-card/80 px-6" data-slot="acme">
        <SettingRow
          label={$t('tls.acme.label')}
          description={$t('tls.acme.help')}
          path="server.tls.acme.enabled"
          restart
          id="acme-enabled"
        >
          <Switch
            id="acme-enabled"
            checked={acme.enabled}
            onCheckedChange={(checked) => set('server.tls.acme.enabled', checked)}
          />
        </SettingRow>

        {#if acme.enabled}
          <SettingRow
            label={$t('tls.acme.provider')}
            description={$t('tls.acme.providerHelp')}
            path="server.tls.acme.directory_url"
            restart
            id="acme-directory"
          >
            <div class="space-y-2">
              <Select.Root
                type="single"
                value={acme.directory_url === LETS_ENCRYPT
                  ? 'production'
                  : acme.directory_url === LETS_ENCRYPT_STAGING
                    ? 'staging'
                    : 'custom'}
                onValueChange={(value) => {
                  if (value === 'production') set('server.tls.acme.directory_url', LETS_ENCRYPT);
                  else if (value === 'staging')
                    set('server.tls.acme.directory_url', LETS_ENCRYPT_STAGING);
                }}
              >
                <Select.Trigger id="acme-directory" class="w-full sm:w-72">
                  {acme.directory_url === LETS_ENCRYPT
                    ? $t('tls.acme.production')
                    : acme.directory_url === LETS_ENCRYPT_STAGING
                      ? $t('tls.acme.staging')
                      : $t('tls.acme.custom')}
                </Select.Trigger>
                <Select.Content>
                  <Select.Item value="staging" label={$t('tls.acme.staging')}>
                    {$t('tls.acme.staging')}
                  </Select.Item>
                  <Select.Item value="production" label={$t('tls.acme.production')}>
                    {$t('tls.acme.production')}
                  </Select.Item>
                  <Select.Item value="custom" label={$t('tls.acme.custom')}>
                    {$t('tls.acme.custom')}
                  </Select.Item>
                </Select.Content>
              </Select.Root>
              <Input
                aria-label={$t('tls.acme.directoryAria')}
                value={acme.directory_url}
                onchange={(event) =>
                  set(
                    'server.tls.acme.directory_url',
                    (event.currentTarget as HTMLInputElement).value.trim()
                  )}
              />
            </div>
          </SettingRow>

          <SettingRow
            label={$t('tls.acme.domains')}
            description={$t('tls.acme.domainsHelp')}
            path="server.tls.acme.domains"
            restart
            id="acme-domains"
          >
            <Input
              id="acme-domains"
              placeholder="example.com, www.example.com"
              value={acme.domains.join(', ')}
              onchange={(event) =>
                set(
                  'server.tls.acme.domains',
                  list((event.currentTarget as HTMLInputElement).value)
                )}
            />
          </SettingRow>

          <SettingRow
            label={$t('tls.acme.email')}
            description={$t('tls.acme.emailHelp')}
            path="server.tls.acme.email"
            restart
            id="acme-email"
          >
            <Input
              id="acme-email"
              placeholder="ops@example.com"
              value={acme.email.join(', ')}
              onchange={(event) =>
                set('server.tls.acme.email', list((event.currentTarget as HTMLInputElement).value))}
            />
          </SettingRow>

          <SettingRow
            label={$t('tls.acme.storage')}
            description={$t('tls.acme.storageHelp')}
            path="server.tls.acme.storage_dir"
            restart
            id="acme-storage"
          >
            <Input
              id="acme-storage"
              value={acme.storage_dir}
              onchange={(event) =>
                set(
                  'server.tls.acme.storage_dir',
                  (event.currentTarget as HTMLInputElement).value.trim()
                )}
            />
          </SettingRow>

          <SettingRow
            label={$t('tls.acme.renewDays')}
            description={$t('tls.acme.renewDaysHelp')}
            path="server.tls.acme.renew_before_days"
            restart
            id="acme-renew-days"
          >
            <Input
              id="acme-renew-days"
              type="number"
              min="1"
              max="89"
              value={acme.renew_before_days}
              onchange={(event) =>
                set(
                  'server.tls.acme.renew_before_days',
                  Number((event.currentTarget as HTMLInputElement).value) || 30
                )}
            />
          </SettingRow>
        {/if}
      </section>

      {#if status?.acme.enabled}
        <section class="rounded-2xl border border-border/80 bg-card/80 p-6">
          <div class="mb-3 flex items-start justify-between gap-4">
            <div>
              <h2 class="text-sm font-semibold text-foreground">
                {$t('tls.acme.running.title')}
              </h2>
              <p class="mt-1 text-xs text-muted-foreground">
                {status.acme.staging
                  ? $t('tls.acme.running.staging')
                  : $t('tls.acme.running.production')} ·
                {status.acme.domains.join(', ') || $t('tls.acme.running.noDomains')}
              </p>
            </div>
            <Button variant="outline" size="sm" onclick={renewNow} disabled={renewing}>
              {renewing ? $t('tls.acme.ordering') : $t('tls.acme.orderNow')}
            </Button>
          </div>

          <dl class="grid gap-x-6 gap-y-1 text-xs sm:grid-cols-2">
            <div class="flex justify-between gap-4 sm:block">
              <dt class="text-muted-foreground">{$t('tls.acme.lastAttempt')}</dt>
              <dd class="text-foreground">{whenever(status.acme.last_attempt_epoch_s)}</dd>
            </div>
            <div class="flex justify-between gap-4 sm:block">
              <dt class="text-muted-foreground">{$t('tls.acme.lastSuccess')}</dt>
              <dd class="text-foreground">{whenever(status.acme.last_success_epoch_s)}</dd>
            </div>
          </dl>

          {#if status.acme.last_error}
            <p
              class="mt-3 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-foreground"
              role="alert"
            >
              {$t('tls.acme.lastError', { error: status.acme.last_error })}
            </p>
          {/if}
        </section>
      {/if}
    {/if}
  {/if}
</div>

<ConfirmDialog
  bind:open={confirmDisableTls}
  title={$t('tls.confirmRemove.title')}
  description={$t('tls.confirmRemove.body')}
  confirmLabel={$t('tls.confirmRemove.confirm')}
  variant="destructive"
  onconfirm={() => {
    void editDraft([{ path: 'server.tls', action: 'remove' }]);
    confirmDisableTls = false;
  }}
/>
