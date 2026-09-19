<script lang="ts">
  /**
   * The draft against what the proxy is running (T307).
   *
   * Two readings of the same diff: unified, which is what anyone who has used
   * `git diff` expects, and side by side, which is easier when a value changed
   * rather than a block moved. Colour is never the only signal — every line
   * carries its `+`/`-` marker and its line numbers.
   */
  import { toHunks, toSideBySide, type DiffOp } from '$lib/configDiff';

  let {
    ops,
    mode = 'unified',
    context = 3,
    beforeLabel = 'Running config',
    afterLabel = 'Draft',
    class: className = ''
  }: {
    ops: DiffOp[];
    mode?: 'unified' | 'split';
    /** Unchanged lines kept around each change in the unified view. */
    context?: number;
    beforeLabel?: string;
    afterLabel?: string;
    class?: string;
  } = $props();

  const hunks = $derived(toHunks(ops, context));
  const rows = $derived(toSideBySide(ops));

  const markerFor = (kind: DiffOp['kind']): string =>
    kind === 'insert' ? '+' : kind === 'delete' ? '-' : ' ';

  const lineTint = (kind: DiffOp['kind']): string =>
    kind === 'insert' ? 'bg-success/10' : kind === 'delete' ? 'bg-destructive/10' : '';

  const markerTint = (kind: DiffOp['kind']): string =>
    kind === 'insert'
      ? 'text-success-emphasis'
      : kind === 'delete'
        ? 'text-destructive-emphasis'
        : 'text-muted-foreground';
</script>

<div class="overflow-hidden rounded-lg border border-border bg-background {className}">
  {#if ops.every((op) => op.kind === 'equal')}
    <p class="px-4 py-6 text-center text-sm text-muted-foreground">
      The draft is identical to the config the proxy is running.
    </p>
  {:else if mode === 'unified'}
    <div class="max-h-[60vh] overflow-auto font-mono text-xs leading-6">
      {#each hunks as hunk (`${hunk.oldStart}-${hunk.newStart}`)}
        <div
          class="sticky top-0 z-10 border-y border-border bg-muted/70 px-3 py-1 text-[11px] font-semibold text-muted-foreground backdrop-blur"
        >
          @@ -{hunk.oldStart},{hunk.oldCount} +{hunk.newStart},{hunk.newCount} @@
        </div>
        {#each hunk.ops as op, index (`${hunk.oldStart}-${hunk.newStart}-${index}`)}
          <div class="flex whitespace-pre-wrap break-all px-1 {lineTint(op.kind)}">
            <span
              class="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground tabular-nums"
              aria-hidden="true">{op.oldLine >= 0 ? op.oldLine + 1 : ''}</span
            >
            <span
              class="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground tabular-nums"
              aria-hidden="true">{op.newLine >= 0 ? op.newLine + 1 : ''}</span
            >
            <span class="w-4 shrink-0 select-none font-semibold {markerTint(op.kind)}"
              >{markerFor(op.kind)}</span
            >
            <span class="flex-1">{op.text || ' '}</span>
          </div>
          {#if op.noNewline}
            <div class="px-1 pl-24 text-[11px] italic text-muted-foreground">
              \ No newline at end of file
            </div>
          {/if}
        {/each}
      {/each}
    </div>
  {:else}
    <div class="max-h-[60vh] overflow-auto">
      <div
        class="sticky top-0 z-10 grid grid-cols-2 border-b border-border bg-muted/70 text-[11px] font-semibold text-muted-foreground backdrop-blur"
      >
        <div class="border-r border-border px-3 py-1">{beforeLabel}</div>
        <div class="px-3 py-1">{afterLabel}</div>
      </div>
      <div class="grid grid-cols-2 font-mono text-xs leading-6">
        {#each rows as row, index (index)}
          <div
            class="flex border-r border-border px-1 whitespace-pre-wrap break-all {row.left ===
            null
              ? 'bg-muted/40'
              : row.kind === 'delete' || row.kind === 'replace'
                ? 'bg-destructive/10'
                : ''}"
          >
            <span
              class="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground tabular-nums"
              aria-hidden="true">{row.left ? row.left.line : ''}</span
            >
            <span class="flex-1">{row.left ? row.left.text || ' ' : ''}</span>
          </div>
          <div
            class="flex px-1 whitespace-pre-wrap break-all {row.right === null
              ? 'bg-muted/40'
              : row.kind === 'insert' || row.kind === 'replace'
                ? 'bg-success/10'
                : ''}"
          >
            <span
              class="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground tabular-nums"
              aria-hidden="true">{row.right ? row.right.line : ''}</span
            >
            <span class="flex-1">{row.right ? row.right.text || ' ' : ''}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
