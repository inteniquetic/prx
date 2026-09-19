<script lang="ts">
  /**
   * The last thing between a draft and the proxy (T307, shared with T308).
   *
   * Whether the change was typed into the editor or made by a form, it is
   * reviewed here first: what it means in words, then the diff itself, in
   * whichever of the two readings suits the change.
   */
  import { Button } from '$lib/components/ui/button';
  import type { ChangeSummary } from '$lib/configChanges';
  import type { DiffOp, DiffStats } from '$lib/configDiff';
  import type { ConfigDiagnostic } from '$lib/api/configText';
  import ChangeSummaryList from './ChangeSummary.svelte';
  import DiffView from './DiffView.svelte';

  let {
    ops,
    summary,
    stats,
    warnings = [],
    applying = false,
    onapply,
    oncancel
  }: {
    ops: DiffOp[];
    summary: ChangeSummary | null;
    stats: DiffStats;
    warnings?: ConfigDiagnostic[];
    applying?: boolean;
    onapply: () => void;
    oncancel: () => void;
  } = $props();

  let mode: 'unified' | 'split' = $state('unified');
</script>

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
          {summary?.headline ?? 'No changes'}
          <span class="mx-1 text-border">·</span>
          <span class="font-mono text-xs">
            <span class="text-success-emphasis">+{stats.added}</span>
            <span class="text-destructive-emphasis">−{stats.removed}</span>
          </span>
        </p>
      </div>
      <div class="flex gap-1 rounded-lg border border-border p-0.5">
        <button
          type="button"
          class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors {mode === 'unified'
            ? 'bg-secondary text-secondary-foreground'
            : 'text-muted-foreground'}"
          onclick={() => (mode = 'unified')}
          aria-pressed={mode === 'unified'}
        >
          Unified
        </button>
        <button
          type="button"
          class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors {mode === 'split'
            ? 'bg-secondary text-secondary-foreground'
            : 'text-muted-foreground'}"
          onclick={() => (mode = 'split')}
          aria-pressed={mode === 'split'}
        >
          Side by side
        </button>
      </div>
    </div>

    {#if warnings.length > 0}
      <div class="rounded-xl border border-warning/40 bg-warning/10 px-4 py-3">
        <p class="text-sm font-medium text-foreground">
          {warnings.length} warning{warnings.length === 1 ? '' : 's'} — these do not stop the
          apply
        </p>
        <ul class="mt-1 space-y-0.5">
          {#each warnings as warning, index (index)}
            <li class="text-xs text-muted-foreground">
              line {warning.line}: {warning.message}
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <ChangeSummaryList {summary} />

    <DiffView {ops} {mode} />

    <div class="flex items-center justify-end gap-2">
      <Button variant="ghost" onclick={oncancel} disabled={applying}>Cancel</Button>
      <Button onclick={onapply} disabled={applying}>
        {applying ? 'Applying…' : 'Apply to the proxy'}
      </Button>
    </div>
  </div>
</div>
