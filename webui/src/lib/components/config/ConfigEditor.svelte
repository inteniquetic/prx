<script lang="ts">
  /**
   * Editing the config file by hand (T307).
   *
   * The rule the whole screen is built around: nothing reaches the proxy
   * without a diff first. Typing is checked against the real validator as it
   * happens, Apply opens the review rather than writing, and a file that
   * changed underneath is a conflict with three sides rather than a silent
   * overwrite.
   */
  import { onMount } from 'svelte';

  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import LoaderIcon from '@lucide/svelte/icons/loader-circle';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import UploadIcon from '@lucide/svelte/icons/upload';

  import { Button } from '$lib/components/ui/button';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { CopyButton } from '$lib/components/ui/copy-button';
  import { toast } from '$lib/components/ui/sonner';
  import type { ConfigDiagnostic } from '$lib/api/configText';
  import {
    applyDraft,
    applying,
    base,
    changeSummary,
    diffOps,
    draft,
    externalEtag,
    isDirty,
    loadBase,
    loading,
    loadError,
    rebaseOnCurrent,
    report,
    restoredAt,
    revertDraft,
    setDraft,
    stats,
    validateNow,
    validating,
    watchExternalChanges
  } from '$lib/stores/configDraft';
  import ChangeSummary from './ChangeSummary.svelte';
  import ConflictDialog from './ConflictDialog.svelte';
  import DiffView from './DiffView.svelte';
  import TomlEditor from './TomlEditor.svelte';

  let { onapplied }: { onapplied?: () => void } = $props();

  let diffMode: 'unified' | 'split' = $state('unified');
  let showReview = $state(false);
  let confirmRevert = $state(false);
  let conflict: { current: string } | null = $state(null);
  let editor: { revealLine: (line: number) => void } | null = $state(null);

  const diagnostics = $derived([...($report?.errors ?? []), ...($report?.warnings ?? [])]);
  const errorCount = $derived($report?.errors.length ?? 0);
  const warningCount = $derived($report?.warnings.length ?? 0);
  /** 1-based lines the draft touched, for the marks in the editor gutter. */
  const changedLines = $derived(
    $diffOps.filter((op) => op.kind === 'insert').map((op) => op.newLine + 1)
  );

  const restoredLabel = $derived(
    $restoredAt ? new Date($restoredAt).toLocaleString() : ''
  );

  onMount(() => {
    void loadBase();
    return watchExternalChanges();
  });

  function jumpTo(diagnostic: ConfigDiagnostic) {
    editor?.revealLine(diagnostic.line);
  }

  async function openReview() {
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
    await loadBase({ discardDraft: !$isDirty });
    if ($isDirty) {
      toast.message('Reloaded — your draft is still here, now compared against the new config');
    }
  }
</script>

<div class="space-y-4">
  <!-- Banners: everything that makes the draft not simply "mine, on top of
       what is running". -->
  {#if $loadError}
    <div
      class="flex items-start gap-2 rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3"
      role="alert"
    >
      <CircleAlertIcon class="mt-0.5 size-4 shrink-0 text-destructive-emphasis" aria-hidden="true" />
      <p class="text-sm text-foreground">{$loadError}</p>
    </div>
  {/if}

  {#if $restoredAt}
    <div
      class="flex items-start gap-2 rounded-xl border border-primary/40 bg-primary/10 px-4 py-3"
      role="status"
    >
      <HistoryIcon class="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
      <p class="text-sm text-foreground">
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
      <TriangleAlertIcon
        class="mt-0.5 size-4 shrink-0 text-warning-emphasis"
        aria-hidden="true"
      />
      <p class="flex-1 text-sm text-foreground">
        The config file changed somewhere else since this draft started. Applying now would
        write over it — reload to see what changed first.
      </p>
      <Button variant="outline" size="sm" onclick={reload}>Reload</Button>
    </div>
  {/if}

  <!-- Toolbar -->
  <div class="flex flex-wrap items-center gap-2">
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
        <span class="font-mono text-xs tabular-nums">
          <span class="text-success-emphasis">+{$stats.added}</span>
          <span class="text-destructive-emphasis">−{$stats.removed}</span>
        </span>
      {/if}
    </div>

    <div class="ms-auto flex items-center gap-2">
      <CopyButton text={$draft} label="Copy TOML" showLabel size="sm" variant="outline" />
      <Button variant="outline" size="sm" onclick={reload} disabled={$loading}>
        {$loading ? 'Loading…' : 'Reload'}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onclick={() => (confirmRevert = true)}
        disabled={!$isDirty}
      >
        <RotateCcwIcon class="size-4" aria-hidden="true" />
        Revert
      </Button>
      <Button size="sm" onclick={openReview} disabled={!$isDirty || errorCount > 0}>
        <UploadIcon class="size-4" aria-hidden="true" />
        Review &amp; apply
      </Button>
    </div>
  </div>

  <!-- Editor -->
  <div class="h-[60vh] min-h-[24rem]">
    <TomlEditor
      bind:this={editor}
      value={$draft}
      diagnostics={diagnostics.map((diagnostic) => ({
        severity: diagnostic.severity,
        message: diagnostic.message,
        hint: diagnostic.hint,
        line: diagnostic.line,
        column: diagnostic.column,
        end_line: diagnostic.end_line,
        end_column: diagnostic.end_column
      }))}
      {changedLines}
      readOnly={$applying}
      onchange={setDraft}
      class="h-full"
    />
  </div>

  <!-- Problems -->
  {#if diagnostics.length > 0}
    <section class="rounded-xl border border-border bg-card/60">
      <h3 class="border-b border-border px-4 py-2 text-sm font-semibold text-foreground">
        Problems
      </h3>
      <ul class="divide-y divide-border/70">
        {#each diagnostics as diagnostic, index (index)}
          <li>
            <button
              type="button"
              class="flex w-full items-start gap-2 px-4 py-2 text-left transition-colors hover:bg-muted/60"
              onclick={() => jumpTo(diagnostic)}
            >
              {#if diagnostic.severity === 'error'}
                <CircleAlertIcon
                  class="mt-0.5 size-4 shrink-0 text-destructive-emphasis"
                  aria-hidden="true"
                />
              {:else}
                <TriangleAlertIcon
                  class="mt-0.5 size-4 shrink-0 text-warning-emphasis"
                  aria-hidden="true"
                />
              {/if}
              <span class="min-w-0 flex-1">
                <span class="text-sm text-foreground">{diagnostic.message}</span>
                {#if diagnostic.hint}
                  <span class="block text-xs text-muted-foreground">{diagnostic.hint}</span>
                {/if}
                <span class="block font-mono text-[11px] text-muted-foreground">
                  line {diagnostic.line}{diagnostic.path ? ` · ${diagnostic.path}` : ''} ·
                  {diagnostic.code}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<!-- Review: the diff and what it means, before anything is written. -->
{#if showReview}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4"
    role="dialog"
    aria-modal="true"
    aria-label="Review config changes"
  >
    <div
      class="flex max-h-[90vh] w-full max-w-5xl flex-col gap-4 overflow-auto rounded-2xl border border-border bg-card p-6 shadow-lg"
    >
      <div class="flex items-start justify-between gap-4">
        <div>
          <h2 class="text-lg font-semibold text-foreground">Review before applying</h2>
          <p class="mt-1 text-sm text-muted-foreground">
            {$changeSummary?.headline ?? 'No changes'}
            <span class="mx-1 text-border">·</span>
            <span class="font-mono text-xs">
              <span class="text-success-emphasis">+{$stats.added}</span>
              <span class="text-destructive-emphasis">−{$stats.removed}</span>
            </span>
          </p>
        </div>
        <div class="flex gap-1 rounded-lg border border-border p-0.5">
          <button
            type="button"
            class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
            class:bg-secondary={diffMode === 'unified'}
            class:text-secondary-foreground={diffMode === 'unified'}
            class:text-muted-foreground={diffMode !== 'unified'}
            onclick={() => (diffMode = 'unified')}
            aria-pressed={diffMode === 'unified'}
          >
            Unified
          </button>
          <button
            type="button"
            class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
            class:bg-secondary={diffMode === 'split'}
            class:text-secondary-foreground={diffMode === 'split'}
            class:text-muted-foreground={diffMode !== 'split'}
            onclick={() => (diffMode = 'split')}
            aria-pressed={diffMode === 'split'}
          >
            Side by side
          </button>
        </div>
      </div>

      {#if warningCount > 0}
        <div class="rounded-xl border border-warning/40 bg-warning/10 px-4 py-3">
          <p class="text-sm font-medium text-foreground">
            {warningCount} warning{warningCount === 1 ? '' : 's'} — these do not stop the apply
          </p>
          <ul class="mt-1 space-y-0.5">
            {#each $report?.warnings ?? [] as warning, index (index)}
              <li class="text-xs text-muted-foreground">
                line {warning.line}: {warning.message}
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <ChangeSummary summary={$changeSummary} />

      <DiffView ops={$diffOps} mode={diffMode} />

      <div class="flex items-center justify-end gap-2">
        <Button variant="ghost" onclick={() => (showReview = false)} disabled={$applying}>
          Cancel
        </Button>
        <Button onclick={apply} disabled={$applying}>
          {$applying ? 'Applying…' : 'Apply to the proxy'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<ConfirmDialog
  bind:open={confirmRevert}
  title="Discard this draft?"
  description="The editor goes back to the config the proxy is running. This cannot be undone."
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
