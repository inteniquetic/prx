<script lang="ts">
  /**
   * The config file, editable by hand (T307).
   *
   * Only the editor and what it has to say about the draft live here: the
   * state of the draft, the review and the apply belong to `DraftBar`, which
   * the Settings page puts above every tab — a form change and a hand edit are
   * the same draft and take the same way out.
   */
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { CopyButton } from '$lib/components/ui/copy-button';
  import type { ConfigDiagnostic } from '$lib/api/configText';
  import { diffOps, draft, report, setDraft } from '$lib/stores/configDraft';
  import TomlEditor from './TomlEditor.svelte';

  let editor: { revealLine: (line: number) => void } | null = $state(null);

  const diagnostics = $derived([...($report?.errors ?? []), ...($report?.warnings ?? [])]);
  /** 1-based lines the draft touched, for the marks in the editor gutter. */
  const changedLines = $derived(
    $diffOps.filter((op) => op.kind === 'insert').map((op) => op.newLine + 1)
  );

  function jumpTo(diagnostic: ConfigDiagnostic) {
    editor?.revealLine(diagnostic.line);
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between gap-2">
    <p class="text-xs text-muted-foreground">
      The file the proxy is running, comments and all. Changes made in the other tabs show up
      here too.
    </p>
    <CopyButton text={$draft} label="Copy TOML" showLabel size="sm" variant="outline" />
  </div>

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
      onchange={setDraft}
      class="h-full"
    />
  </div>

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
