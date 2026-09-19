<script lang="ts">
  /**
   * Certificates as a page of their own (T308).
   *
   * The same panel the Settings page shows under its TLS tab. It is worth its
   * own entry in the sidebar because "when does this expire" is a question
   * people come to the UI with, rather than one they arrive at by way of
   * settings.
   */
  import DraftBar from '../config/DraftBar.svelte';
  import TlsSettings from '../settings/TlsSettings.svelte';
  import { t } from '$lib/i18n';

  let { onapplied }: { onapplied?: () => void } = $props();
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-4 sm:px-6"
  >
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold">{$t('tlsPage.title')}</h1>
      <p class="mt-0.5 text-sm text-muted-foreground">{$t('tlsPage.subtitle')}</p>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
    <DraftBar {onapplied} />
    <div class="mt-4 pb-10">
      <TlsSettings />
    </div>
  </div>
</div>
