<script lang="ts">
  /**
   * The config file, editable (T307).
   *
   * CodeMirror is loaded with a dynamic import when this component mounts, so
   * it is fetched the first time someone opens the TOML tab and never on any
   * other page. Until it arrives — and if it fails to arrive at all — a plain
   * textarea holds the same text, because an editor that does not load must not
   * mean a config that cannot be edited.
   */
  import { onMount, untrack } from 'svelte';

  import type { EditorDiagnostic, EditorHandle } from './editorSetup';

  let {
    value,
    diagnostics = [],
    changedLines = [],
    readOnly = false,
    onchange,
    class: className = ''
  }: {
    value: string;
    diagnostics?: EditorDiagnostic[];
    /** 1-based lines the draft has touched, marked in the gutter. */
    changedLines?: number[];
    readOnly?: boolean;
    onchange: (value: string) => void;
    class?: string;
  } = $props();

  let host: HTMLDivElement | null = $state(null);
  let handle: EditorHandle | null = $state(null);
  let failed = $state('');
  let cursor = $state({ line: 1, column: 1 });

  export function revealLine(line: number): void {
    handle?.revealLine(line);
  }

  export function focusEditor(): void {
    handle?.focus();
  }

  onMount(() => {
    let disposed = false;

    void (async () => {
      try {
        const module = await import('./editorSetup');
        if (disposed || !host) return;
        handle = module.createEditor({
          parent: host,
          doc: untrack(() => value),
          readOnly: untrack(() => readOnly),
          onChange: onchange,
          onCursor: (line, column) => {
            cursor = { line, column };
          }
        });
        handle.setDiagnostics(untrack(() => diagnostics));
        handle.markChangedLines(untrack(() => changedLines));
      } catch (error) {
        failed = error instanceof Error ? error.message : String(error);
      }
    })();

    return () => {
      disposed = true;
      handle?.destroy();
      handle = null;
    };
  });

  // The store is the source of truth: a value that changed elsewhere (a reload,
  // a reverted draft, the other side of a conflict) is pushed into the editor,
  // which ignores it when the text already matches.
  $effect(() => {
    const next = value;
    untrack(() => handle)?.setValue(next);
  });

  $effect(() => {
    const next = diagnostics;
    untrack(() => handle)?.setDiagnostics(next);
  });

  $effect(() => {
    const next = changedLines;
    untrack(() => handle)?.markChangedLines(next);
  });

  $effect(() => {
    const next = readOnly;
    untrack(() => handle)?.setReadOnly(next);
  });
</script>

<div class="flex h-full min-h-0 flex-col {className}">
  {#if failed}
    <div
      class="mb-2 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-xs text-warning-foreground"
      role="status"
    >
      The rich editor could not be loaded ({failed}). The plain editor below
      still applies the same validation and diff.
    </div>
  {/if}

  {#if handle === null}
    <!-- Also the fallback: everything that matters — typing, validation, the
         diff and Apply — works without CodeMirror. -->
    <textarea
      class="h-full min-h-[24rem] w-full resize-none rounded-lg border border-border bg-background p-3 font-mono text-[0.8125rem] leading-relaxed text-foreground outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
      spellcheck="false"
      aria-label="Config file"
      readonly={readOnly}
      {value}
      oninput={(event) => onchange((event.currentTarget as HTMLTextAreaElement).value)}
    ></textarea>
  {/if}

  <div
    bind:this={host}
    class="min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background"
    class:hidden={handle === null}
    data-slot="toml-editor"
  ></div>

  <p class="mt-1.5 text-right text-[11px] tabular-nums text-muted-foreground">
    Line {cursor.line}, column {cursor.column}
    <span class="mx-1 text-border">·</span>
    <kbd class="rounded border border-border px-1">Ctrl</kbd>+<kbd
      class="rounded border border-border px-1">F</kbd
    > to search
  </p>
</div>
