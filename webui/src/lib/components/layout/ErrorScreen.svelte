<script lang="ts">
  /**
   * What a crash looks like (T309).
   *
   * A blank page with a stack trace in the console tells an operator nothing
   * they can act on. This says what happened, what is unaffected — the proxy
   * keeps serving traffic whatever this page does — and the two things worth
   * trying, with the details kept for whoever files the bug.
   */
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';

  import { Button } from '$lib/components/ui/button';
  import { t } from '$lib/i18n';

  let {
    error,
    reset
  }: {
    error: unknown;
    reset?: () => void;
  } = $props();

  const detail = $derived(
    error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  );
</script>

<div class="flex min-h-[60vh] items-center justify-center p-6" role="alert" data-slot="error-screen">
  <div class="max-w-xl rounded-2xl border border-destructive/40 bg-destructive/5 p-6">
    <h1 class="flex items-center gap-2 text-lg font-semibold text-foreground">
      <CircleAlertIcon class="size-5 text-destructive-emphasis" aria-hidden="true" />
      {$t('error.title')}
    </h1>
    <p class="mt-2 text-sm text-muted-foreground">{$t('error.proxyUnaffected')}</p>
    <p class="mt-2 text-sm text-muted-foreground">{$t('error.whatToDo')}</p>

    <div class="mt-4 flex flex-wrap gap-2">
      {#if reset}
        <Button size="sm" onclick={reset}>{$t('error.retry')}</Button>
      {/if}
      <Button variant="outline" size="sm" onclick={() => window.location.reload()}>
        {$t('error.reload')}
      </Button>
    </div>

    <details class="mt-4">
      <summary class="cursor-pointer text-xs text-muted-foreground">{$t('error.details')}</summary>
      <pre
        class="mt-2 overflow-auto rounded-lg border border-border bg-background p-3 font-mono text-[11px] text-foreground">{detail}</pre>
    </details>
  </div>
</div>
