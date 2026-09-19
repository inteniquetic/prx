<script lang="ts">
  /**
   * Everything the proxy is, other than its routes and services (T308).
   *
   * The forms and the TOML editor are two views of one draft: a switch flipped
   * here changes the same file the editor is showing, key by key, and both take
   * the same way out — a diff, then Apply. That is why the draft bar sits above
   * the tabs rather than inside one of them.
   */
  import DownloadIcon from '@lucide/svelte/icons/download';
  import UploadIcon from '@lucide/svelte/icons/upload';

  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import ConfigEditor from '../config/ConfigEditor.svelte';
  import DraftBar from '../config/DraftBar.svelte';
  import AdminSettings from '../settings/AdminSettings.svelte';
  import ObservabilitySettings from '../settings/ObservabilitySettings.svelte';
  import ServerSettings from '../settings/ServerSettings.svelte';
  import TlsSettings from '../settings/TlsSettings.svelte';
  import { encodeToml } from '$lib/configCodec';
  import { normalizePrxConfig } from '$lib/configNormalize';
  import { draftConfig, setDraft } from '$lib/stores/configDraft';
  import type { PrxConfig } from '$lib/types/config';
  import { t } from '$lib/i18n';

  let {
    /** Bumped by the shell to open the review from the command palette. */
    reviewRequest = 0,
    onapplied
  }: {
    reviewRequest?: number;
    onapplied?: () => void;
  } = $props();

  type SettingsTab = 'server' | 'tls' | 'observability' | 'admin' | 'toml';

  const tabs: SettingsTab[] = ['server', 'tls', 'observability', 'admin', 'toml'];

  let activeTab: SettingsTab = $state('server');
  let importInput: HTMLInputElement | null = $state(null);

  const config = $derived($draftConfig);

  function exportJson() {
    if (!config) return;
    const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' });
    const href = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = 'prx-config.json';
    anchor.click();
    URL.revokeObjectURL(href);
  }

  /**
   * Imports a config exported from somewhere else.
   *
   * This is the one path that renders a whole file rather than editing keys —
   * a config from another machine has no comments of ours to keep — so it lands
   * in the draft as a diff like everything else, to be read before it is
   * applied.
   */
  async function importJson(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    try {
      const parsed = JSON.parse(await file.text()) as Partial<PrxConfig>;
      setDraft(encodeToml(normalizePrxConfig(parsed)));
      activeTab = 'toml';
      toast.message($t('settings.importToast'));
    } catch (error) {
      toast.error(
        $t('settings.importFailed', {
          error: error instanceof Error ? error.message : String(error)
        })
      );
    } finally {
      input.value = '';
    }
  }

  const tabClass = (tab: SettingsTab): string =>
    `-mb-px border-b-2 px-4 py-2 text-sm font-medium transition-colors ${
      activeTab === tab
        ? 'border-primary text-foreground'
        : 'border-transparent text-muted-foreground hover:text-foreground'
    }`;
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">{$t('settings.title')}</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">{$t('settings.subtitle')}</p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={exportJson} disabled={!config}>
        <DownloadIcon aria-hidden="true" />
        <span class="hidden sm:inline">{$t('settings.exportJson')}</span>
      </Button>
      <Button variant="outline" size="sm" onclick={() => importInput?.click()}>
        <UploadIcon aria-hidden="true" />
        <span class="hidden sm:inline">{$t('settings.importJson')}</span>
      </Button>
      <input
        bind:this={importInput}
        type="file"
        accept=".json,application/json"
        class="hidden"
        aria-label={$t('settings.importAria')}
        onchange={importJson}
      />
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
    <DraftBar openReviewRequest={reviewRequest} {onapplied} />

    <nav
      class="mt-4 flex gap-0 overflow-x-auto border-b border-border/80"
      aria-label={$t('settings.sections')}
    >
      {#each tabs as tab (tab)}
        <button
          type="button"
          class={tabClass(tab)}
          aria-current={activeTab === tab ? 'page' : undefined}
          onclick={() => (activeTab = tab)}
        >
          {$t(`settings.tab.${tab}`)}
        </button>
      {/each}
    </nav>

    <div class="mt-6 pb-10">
      {#if activeTab === 'server'}
        <ServerSettings />
      {:else if activeTab === 'tls'}
        <TlsSettings />
      {:else if activeTab === 'observability'}
        <ObservabilitySettings />
      {:else if activeTab === 'admin'}
        <AdminSettings />
      {:else}
        <ConfigEditor />
      {/if}
    </div>
  </div>
</div>
