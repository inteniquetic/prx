<script lang="ts">
  /**
   * The state of the draft, and the only way out of it (T307, T308).
   *
   * Every page that changes the config — the TOML editor and the Settings forms
   * alike — puts this at the top. It says whether the draft is valid, how much
   * it changes, and whether anybody else has touched the file; and it owns the
   * review, the apply and the conflict, so there is exactly one way a change
   * reaches the proxy.
   */
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import LoaderIcon from '@lucide/svelte/icons/loader-circle';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import UploadIcon from '@lucide/svelte/icons/upload';

  import { Button } from '$lib/components/ui/button';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { toast } from '$lib/components/ui/sonner';
  import {
    applyDraft,
    applying,
    base,
    changeSummary,
    diffOps,
    draft,
    editError,
    externalEtag,
    isDirty,
    loadBase,
    loading,
    loadError,
    rebaseOnCurrent,
    report,
    restoredAt,
    revertDraft,
    stats,
    validateNow,
    validating
  } from '$lib/stores/configDraft';
  import ConflictDialog from './ConflictDialog.svelte';
  import ReviewDialog from './ReviewDialog.svelte';

  let {
    /** Called after a successful apply, e.g. to refresh other views. */
    onapplied,
    /** Opened from elsewhere, such as the command palette. */
    openReviewRequest = 0
  }: {
    onapplied?: () => void;
    openReviewRequest?: number;
  } = $props();

  let showReview = $state(false);
  let confirmRevert = $state(false);
  let conflict: { current: string } | null = $state(null);

  const errorCount = $derived($report?.errors.length ?? 0);
  const warningCount = $derived($report?.warnings.length ?? 0);
  const restoredLabel = $derived($restoredAt ? new Date($restoredAt).toLocaleString() : '');

  let lastReviewRequest = $state(0);
  $effect(() => {
    if (openReviewRequest > lastReviewRequest) {
      lastReviewRequest = openReviewRequest;
      void openReview();
    }
  });

  export async function openReview() {
    if (!$isDirty) {
      toast.message('Nothing to apply — the draft matches the running config');
      return;
    }
    // Whatever is on screen is checked before the review opens, so the diff is
    // never reviewed against a stale verdict.
    await validateNow();
    if (($report?.errors.length ?? 0) > 0) {
      toast.error('Fix the errors before applying');
      return;
    }
    showReview = true;
  }

  async function apply() {
    const result = await applyDraft();

    if (result.status === 'applied') {
      showReview = false;
      toast.success('Config applied');
      onapplied?.();
      return;
    }

    if (result.status === 'conflict') {
      showReview = false;
      conflict = { current: result.currentToml };
      return;
    }

    toast.error(result.message);
  }

  async function keepMine() {
    if (!conflict) return;
    // The draft keeps its lines, but the diff and the If-Match now describe
    // what the proxy is really running.
    await rebaseOnCurrent();
    conflict = null;
    await apply();
  }

  async function takeTheirs() {
    conflict = null;
    await loadBase({ discardDraft: true });
    toast.message('Loaded the config the proxy is running');
  }

  async function reload() {
    const wasDirty = $isDirty;
    await loadBase({ discardDraft: !wasDirty });
    if (wasDirty) {
      toast.message('Reloaded — your draft is still here, now compared against the new config');
    }
  }
</script>

<div class="space-y-3">
  {#if $loadError}
    <div
      class="flex items-start gap-2 rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3"
      role="alert"
    >
      <CircleAlertIcon
        class="mt-0.5 size-4 shrink-0 text-destructive-emphasis"
        aria-hidden="true"
      />
      <p class="text-sm text-foreground">{$loadError}</p>
    </div>
  {/if}

  {#if $editError}
    <div
      class="flex items-start gap-2 rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3"
      role="alert"
    >
      <CircleAlertIcon
        class="mt-0.5 size-4 shrink-0 text-destructive-emphasis"
        aria-hidden="true"
      />
      <p class="text-sm text-foreground">
        {$editError}
      </p>
    </div>
  {/if}

  {#if $restoredAt}
    <div
      class="flex items-start gap-2 rounded-xl border border-primary/40 bg-primary/10 px-4 py-3"
      role="status"
    >
      <HistoryIcon class="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
      <p class="flex-1 text-sm text-foreground">
        Draft restored from {restoredLabel}. It has not been applied — the proxy is still
        running the config on the left of the diff.
      </p>
      <Button variant="ghost" size="sm" onclick={() => (confirmRevert = true)}>Discard</Button>
    </div>
  {/if}

  {#if $externalEtag}
    <div
      class="flex items-start gap-2 rounded-xl border border-warning/40 bg-warning/10 px-4 py-3"
      role="alert"
    >
      <TriangleAlertIcon class="mt-0.5 size-4 shrink-0 text-warning-emphasis" aria-hidden="true" />
      <p class="flex-1 text-sm text-foreground">
        The config file changed somewhere else since this draft started. Applying now would
        write over it — reload to see what changed first.
      </p>
      <Button variant="outline" size="sm" onclick={reload}>Reload</Button>
    </div>
  {/if}

  <div class="flex flex-wrap items-center gap-2" data-slot="draft-bar">
    <div class="flex items-center gap-2 text-sm">
      {#if $validating}
        <LoaderIcon class="size-4 animate-spin text-muted-foreground" aria-hidden="true" />
        <span class="text-muted-foreground">Checking…</span>
      {:else if errorCount > 0}
        <CircleAlertIcon class="size-4 text-destructive-emphasis" aria-hidden="true" />
        <span class="font-medium text-destructive-emphasis">
          {errorCount} error{errorCount === 1 ? '' : 's'}
        </span>
      {:else}
        <CircleCheckIcon class="size-4 text-success-emphasis" aria-hidden="true" />
        <span class="font-medium text-success-emphasis">Valid</span>
      {/if}
      {#if warningCount > 0}
        <span class="text-muted-foreground">·</span>
        <span class="text-warning-emphasis">
          {warningCount} warning{warningCount === 1 ? '' : 's'}
        </span>
      {/if}
      {#if $isDirty}
        <span class="text-muted-foreground">·</span>
        <span class="font-mono text-xs tabular-nums" data-slot="draft-stats">
          <span class="text-success-emphasis">+{$stats.added}</span>
          <span class="text-destructive-emphasis">−{$stats.removed}</span>
        </span>
        <span class="text-xs text-muted-foreground">not applied yet</span>
      {/if}
    </div>

    <div class="ms-auto flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={reload} disabled={$loading}>
        {$loading ? 'Loading…' : 'Reload'}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => (confirmRevert = true)} disabled={!$isDirty}>
        <RotateCcwIcon class="size-4" aria-hidden="true" />
        Revert
      </Button>
      <Button size="sm" onclick={openReview} disabled={!$isDirty || errorCount > 0}>
        <UploadIcon class="size-4" aria-hidden="true" />
        Review &amp; apply
      </Button>
    </div>
  </div>
</div>

{#if showReview}
  <ReviewDialog
    ops={$diffOps}
    summary={$changeSummary}
    stats={$stats}
    warnings={$report?.warnings ?? []}
    applying={$applying}
    onapply={apply}
    oncancel={() => (showReview = false)}
  />
{/if}

<ConfirmDialog
  bind:open={confirmRevert}
  title="Discard this draft?"
  description="Everything goes back to the config the proxy is running — both the fields changed in these forms and anything typed into the editor. This cannot be undone."
  confirmLabel="Discard draft"
  variant="destructive"
  onconfirm={() => {
    revertDraft();
    confirmRevert = false;
  }}
/>

{#if conflict}
  <ConflictDialog
    open={true}
    baseToml={$base.toml}
    currentToml={conflict.current}
    draftToml={$draft}
    applying={$applying}
    onkeepMine={keepMine}
    ontakeTheirs={takeTheirs}
    oncancel={() => (conflict = null)}
  />
{/if}
