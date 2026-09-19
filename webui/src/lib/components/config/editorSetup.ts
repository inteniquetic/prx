/**
 * The CodeMirror editor for the config file (T307).
 *
 * This module is imported dynamically by `TomlEditor.svelte`, which is what
 * keeps CodeMirror out of the bundle every other page pays for: it is fetched
 * when someone opens the TOML tab and never before. `scripts/bundle-budget.mjs`
 * keeps it honest by budgeting this chunk separately.
 *
 * The TOML mode is written here rather than pulled from `@codemirror/legacy-modes`:
 * that one reports three token types, and a config file is much easier to read
 * when a table header, a key, a string and a number each look different.
 */

import {
  Compartment,
  EditorState,
  StateEffect,
  StateField,
  type Extension,
  type Range
} from '@codemirror/state';
import {
  Decoration,
  EditorView,
  ViewPlugin,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
  tooltips,
  type DecorationSet
} from '@codemirror/view';
import {
  HighlightStyle,
  StreamLanguage,
  bracketMatching,
  codeFolding,
  foldGutter,
  foldKeymap,
  foldService,
  indentOnInput,
  syntaxHighlighting,
  type StreamParser
} from '@codemirror/language';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint';
import { tags } from '@lezer/highlight';

export interface EditorDiagnostic {
  severity: 'error' | 'warning';
  message: string;
  /** 1-based, as the admin API reports them. */
  line: number;
  column: number;
  end_line: number;
  end_column: number;
  hint?: string;
}

export interface EditorHandle {
  /** Replaces the text without losing the cursor when it is only a reload. */
  setValue(text: string): void;
  setDiagnostics(diagnostics: EditorDiagnostic[]): void;
  setReadOnly(readOnly: boolean): void;
  /** Draws a bar next to every line the draft changed. */
  markChangedLines(lines: number[]): void;
  /** Puts the cursor on a line and scrolls it into view. */
  revealLine(line: number): void;
  focus(): void;
  destroy(): void;
}

// ---------------------------------------------------------------------------
// TOML mode
// ---------------------------------------------------------------------------

interface TomlState {
  /** True once past the `=` on a line: `[` then means an array, not a table. */
  inValue: boolean;
  /** The delimiter of the multi-line string being scanned, if any. */
  multiline: string | null;
}

const NUMBER =
  /^[-+]?(0x[0-9a-fA-F_]+|0o[0-7_]+|0b[01_]+|(\d[\d_]*)(\.[\d_]+)?([eE][-+]?\d[\d_]*)?|inf|nan)/;
const DATE = /^\d{4}-\d{2}-\d{2}([Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[-+]\d{2}:\d{2})?)?/;
const BARE_KEY = /^[A-Za-z0-9_-]+/;

const tomlMode: StreamParser<TomlState> = {
  name: 'toml',
  startState: () => ({ inValue: false, multiline: null }),

  token(stream, state) {
    if (state.multiline) {
      const delimiter = state.multiline;
      while (!stream.eol()) {
        if (stream.match(delimiter)) {
          state.multiline = null;
          break;
        }
        stream.next();
      }
      if (stream.eol() && state.multiline) stream.skipToEnd();
      return 'string';
    }

    if (stream.sol()) state.inValue = false;
    if (stream.eatSpace()) return null;

    if (stream.peek() === '#') {
      stream.skipToEnd();
      return 'comment';
    }

    // `[table]` and `[[array.of.tables]]` — the structure of the file, and the
    // thing the eye looks for when scrolling.
    if (!state.inValue && stream.match(/^\[\[?[^\]\n]*\]?\]?/)) return 'typeName';

    if (stream.match(/^('''|""")/)) {
      state.multiline = stream.current();
      return 'string';
    }

    const quote = stream.match(/^['"]/) ? stream.current() : null;
    if (quote) {
      let escaped = false;
      while (!stream.eol()) {
        const next = stream.next();
        if (!escaped && next === quote) break;
        escaped = !escaped && next === '\\' && quote === '"';
      }
      return state.inValue ? 'string' : 'propertyName';
    }

    if (stream.match(/^=/)) {
      state.inValue = true;
      return 'operator';
    }

    if (state.inValue) {
      if (stream.match(/^(true|false)\b/)) return 'bool';
      if (stream.match(DATE)) return 'number';
      if (stream.match(NUMBER)) return 'number';
      if (stream.match(/^[[\]{},]/)) return 'punctuation';
      if (stream.match(BARE_KEY)) return 'variableName';
      stream.next();
      return null;
    }

    if (stream.match(BARE_KEY)) return 'propertyName';
    if (stream.match(/^\./)) return 'punctuation';

    stream.next();
    return null;
  }
};

const tomlLanguage = StreamLanguage.define(tomlMode);

/**
 * Folds a table header down to the line before the next one.
 *
 * `[[service]]` with four upstreams under it is a screenful; being able to
 * collapse it is the difference between reading the file and scrolling it.
 */
const tomlFolding = foldService.of((state, lineStart, lineEnd) => {
  const header = state.doc.lineAt(lineStart);
  if (!/^\s*\[/.test(header.text)) return null;

  const nested = /^\s*\[\[/.test(header.text);
  for (let number = header.number + 1; number <= state.doc.lines; number += 1) {
    const line = state.doc.line(number);
    // A `[[service.upstream]]` belongs to the `[[service]]` above it; another
    // `[[service]]` does not.
    if (/^\s*\[/.test(line.text)) {
      const isChild = nested && line.text.trimStart().startsWith(`[${header.text.trim().slice(1, -1)}.`);
      if (!isChild) {
        return number - 1 > header.number ? { from: lineEnd, to: state.doc.line(number - 1).to } : null;
      }
    }
  }
  return state.doc.lines > header.number ? { from: lineEnd, to: state.doc.line(state.doc.lines).to } : null;
});

const highlightStyle = HighlightStyle.define([
  { tag: tags.comment, color: 'var(--color-muted-foreground)', fontStyle: 'italic' },
  { tag: tags.typeName, color: 'var(--color-primary)', fontWeight: '600' },
  { tag: tags.propertyName, color: 'var(--color-foreground)' },
  { tag: tags.string, color: 'var(--color-success-emphasis)' },
  { tag: tags.number, color: 'var(--color-circuit-emphasis)' },
  { tag: tags.bool, color: 'var(--color-warning-emphasis)', fontWeight: '600' },
  { tag: tags.operator, color: 'var(--color-muted-foreground)' },
  { tag: tags.punctuation, color: 'var(--color-muted-foreground)' },
  { tag: tags.variableName, color: 'var(--color-foreground)' }
]);

/** Everything visual comes from the app's own tokens, so dark mode is free. */
const theme = EditorView.theme({
  '&': {
    color: 'var(--color-foreground)',
    backgroundColor: 'transparent',
    fontSize: '0.8125rem',
    height: '100%'
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.6',
    overflow: 'auto'
  },
  '.cm-content': { caretColor: 'var(--color-primary)', padding: '0.5rem 0' },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    color: 'var(--color-muted-foreground)',
    borderRight: '1px solid var(--color-border)'
  },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--color-foreground)' },
  '.cm-activeLine': { backgroundColor: 'color-mix(in oklab, var(--color-muted) 45%, transparent)' },
  '.cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'color-mix(in oklab, var(--color-primary) 25%, transparent)'
  },
  '&.cm-focused .cm-selectionBackground': {
    backgroundColor: 'color-mix(in oklab, var(--color-primary) 30%, transparent)'
  },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--color-primary)' },
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in oklab, var(--color-warning) 35%, transparent)'
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in oklab, var(--color-warning) 60%, transparent)'
  },
  '.cm-selectionMatch': {
    backgroundColor: 'color-mix(in oklab, var(--color-primary) 18%, transparent)'
  },
  '.cm-panels': {
    backgroundColor: 'var(--color-card)',
    color: 'var(--color-foreground)',
    border: '1px solid var(--color-border)'
  },
  '.cm-panel input, .cm-panel button, .cm-panel label': {
    fontFamily: 'var(--font-sans)',
    fontSize: '0.75rem'
  },
  '.cm-panel input[type=text]': {
    backgroundColor: 'var(--color-background)',
    color: 'var(--color-foreground)',
    border: '1px solid var(--color-border)',
    borderRadius: '0.375rem',
    padding: '0.125rem 0.375rem'
  },
  '.cm-panel button': {
    backgroundColor: 'var(--color-secondary)',
    color: 'var(--color-secondary-foreground)',
    border: '1px solid var(--color-border)',
    borderRadius: '0.375rem',
    padding: '0.125rem 0.5rem',
    cursor: 'pointer'
  },
  '.cm-tooltip': {
    backgroundColor: 'var(--color-popover)',
    color: 'var(--color-popover-foreground)',
    border: '1px solid var(--color-border)',
    borderRadius: '0.5rem',
    fontFamily: 'var(--font-sans)',
    fontSize: '0.75rem',
    maxWidth: '32rem',
    padding: '0.25rem'
  },
  '.cm-diagnostic': { padding: '0.25rem 0.5rem', borderLeft: 'none' },
  '.cm-diagnostic-error': { borderLeft: '3px solid var(--color-destructive)' },
  '.cm-diagnostic-warning': { borderLeft: '3px solid var(--color-warning)' },
  '.cm-lintRange-error': {
    // A wavy underline in the same red the rest of the UI uses for errors.
    backgroundImage: 'none',
    textDecoration: 'underline wavy var(--color-destructive)',
    textDecorationSkipInk: 'none'
  },
  '.cm-lintRange-warning': {
    backgroundImage: 'none',
    textDecoration: 'underline wavy var(--color-warning-emphasis)',
    textDecorationSkipInk: 'none'
  },
  '.cm-foldPlaceholder': {
    backgroundColor: 'var(--color-muted)',
    color: 'var(--color-muted-foreground)',
    border: '1px solid var(--color-border)',
    borderRadius: '0.375rem',
    padding: '0 0.375rem',
    margin: '0 0.25rem'
  }
});

// ---------------------------------------------------------------------------
// Changed-line marks
// ---------------------------------------------------------------------------

const setChangedLines = StateEffect.define<number[]>();

const changedLineMark = Decoration.line({ class: 'cm-prx-changed' });

/** A bar in the gutter margin on every line the draft has touched. */
const changedLines = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (!effect.is(setChangedLines)) continue;
      const marks: Range<Decoration>[] = [];
      for (const line of effect.value) {
        if (line >= 1 && line <= transaction.state.doc.lines) {
          marks.push(changedLineMark.range(transaction.state.doc.line(line).from));
        }
      }
      return Decoration.set(marks, true);
    }
    return transaction.docChanged ? value.map(transaction.changes) : value;
  },
  provide: (field) => EditorView.decorations.from(field)
});

const changedLineTheme = EditorView.baseTheme({
  '.cm-prx-changed': {
    boxShadow: 'inset 2px 0 0 0 var(--color-primary)',
    backgroundColor: 'color-mix(in oklab, var(--color-primary) 7%, transparent)'
  }
});

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

export interface CreateEditorOptions {
  parent: HTMLElement;
  doc: string;
  readOnly?: boolean;
  onChange: (value: string) => void;
  /** Called with the 1-based line when the cursor moves, for the status bar. */
  onCursor?: (line: number, column: number) => void;
}

/** Turns a 1-based line/column from the API into a document offset. */
function offsetOf(state: EditorState, line: number, column: number): number {
  const clamped = Math.min(Math.max(line, 1), state.doc.lines);
  const target = state.doc.line(clamped);
  return Math.min(target.from + Math.max(column - 1, 0), target.to);
}

function toCodeMirrorDiagnostics(
  state: EditorState,
  diagnostics: EditorDiagnostic[]
): Diagnostic[] {
  return diagnostics.map((diagnostic) => {
    const from = offsetOf(state, diagnostic.line, diagnostic.column);
    const line = state.doc.lineAt(from);
    let to = offsetOf(state, diagnostic.end_line, diagnostic.end_column);
    // A zero-width mark is invisible, so an error with no range underlines the
    // rest of its line instead.
    if (to <= from) to = Math.max(line.to, from + 1);
    return {
      from,
      to: Math.min(to, state.doc.length),
      severity: diagnostic.severity,
      message: diagnostic.hint
        ? `${diagnostic.message}\n${diagnostic.hint}`
        : diagnostic.message
    };
  });
}

export function createEditor(options: CreateEditorOptions): EditorHandle {
  // Read-only is toggled while applying, so it lives in its own compartment
  // rather than being baked into the initial configuration.
  const editable = new Compartment();

  const updateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged) options.onChange(update.state.doc.toString());
    if (update.selectionSet && options.onCursor) {
      const head = update.state.selection.main.head;
      const line = update.state.doc.lineAt(head);
      options.onCursor(line.number, head - line.from + 1);
    }
  });

  const extensions: Extension[] = [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightSpecialChars(),
    history(),
    drawSelection(),
    dropCursor(),
    EditorState.allowMultipleSelections.of(true),
    indentOnInput(),
    bracketMatching(),
    codeFolding(),
    foldGutter(),
    tomlFolding,
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    search({ top: true }),
    lintGutter(),
    tooltips({ parent: document.body }),
    changedLines,
    changedLineTheme,
    keymap.of([...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap, indentWithTab]),
    tomlLanguage,
    syntaxHighlighting(highlightStyle),
    theme,
    EditorView.lineWrapping,
    editable.of(readOnlyExtension(options.readOnly ?? false)),
    updateListener
  ];

  const view = new EditorView({
    state: EditorState.create({ doc: options.doc, extensions }),
    parent: options.parent
  });

  return {
    setValue(text: string) {
      if (text === view.state.doc.toString()) return;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text },
        // Keep the cursor inside the new document rather than wherever it was.
        selection: { anchor: Math.min(view.state.selection.main.anchor, text.length) }
      });
    },
    setDiagnostics(diagnostics: EditorDiagnostic[]) {
      view.dispatch(setDiagnostics(view.state, toCodeMirrorDiagnostics(view.state, diagnostics)));
    },
    setReadOnly(next: boolean) {
      view.dispatch({ effects: editable.reconfigure(readOnlyExtension(next)) });
    },
    markChangedLines(lines: number[]) {
      view.dispatch({ effects: setChangedLines.of(lines) });
    },
    revealLine(line: number) {
      const clamped = Math.min(Math.max(line, 1), view.state.doc.lines);
      const target = view.state.doc.line(clamped);
      view.dispatch({
        selection: { anchor: target.from },
        effects: EditorView.scrollIntoView(target.from, { y: 'center' })
      });
      view.focus();
    },
    focus() {
      view.focus();
    },
    destroy() {
      view.destroy();
    }
  };
}

function readOnlyExtension(readOnly: boolean): Extension {
  return [EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)];
}
