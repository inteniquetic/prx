/**
 * Line diff for the config editor (T307).
 *
 * Nobody should apply a config without seeing what they are about to change,
 * and a diff is only worth showing if it is the same diff `diff -u` would
 * print. This is a plain LCS: it finds a minimal edit script, and the change
 * blocks are then slid as far down as they can go, which is the convention GNU
 * diff follows when several minimal scripts exist.
 *
 * `npm run diff:check` compares the output against the system `diff -u` on
 * generated cases, so "the same" is measured rather than assumed.
 */

export type DiffKind = 'equal' | 'insert' | 'delete';

export interface DiffOp {
  kind: DiffKind;
  /** 0-based line number in the old text, or -1 for an insertion. */
  oldLine: number;
  /** 0-based line number in the new text, or -1 for a deletion. */
  newLine: number;
  text: string;
  /** Last line of a file that does not end with a newline. */
  noNewline?: boolean;
}

export interface DiffStats {
  added: number;
  removed: number;
  /** True when the two texts are identical. */
  same: boolean;
}

/** Splits text into lines the way a line-oriented diff sees them. */
export function splitLines(text: string): string[] {
  if (text === '') return [];
  const lines = text.split('\n');
  // A trailing newline ends the last line; it does not start an empty one.
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/**
 * A minimal edit script turning `before` into `after`.
 *
 * Common head and tail are matched off first, which is what keeps this cheap
 * on the usual case: a config file where one value changed.
 */
export function diffLines(before: string[], after: string[]): DiffOp[] {
  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) head += 1;

  let tail = 0;
  while (
    tail < before.length - head &&
    tail < after.length - head &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  ) {
    tail += 1;
  }

  const ops: DiffOp[] = [];
  for (let index = 0; index < head; index += 1) {
    ops.push({ kind: 'equal', oldLine: index, newLine: index, text: before[index] });
  }

  const midBefore = before.slice(head, before.length - tail);
  const midAfter = after.slice(head, after.length - tail);
  ops.push(...lcsOps(midBefore, midAfter, head, head));

  for (let index = 0; index < tail; index += 1) {
    const oldLine = before.length - tail + index;
    ops.push({
      kind: 'equal',
      oldLine,
      newLine: after.length - tail + index,
      text: before[oldLine]
    });
  }

  return slideChangesDown(ops);
}

/** Classic O(n·m) LCS table. Config files are small; clarity wins here. */
function lcsOps(before: string[], after: string[], oldBase: number, newBase: number): DiffOp[] {
  const rows = before.length;
  const cols = after.length;

  if (rows === 0 || cols === 0) {
    const ops: DiffOp[] = [];
    for (let index = 0; index < rows; index += 1) {
      ops.push({ kind: 'delete', oldLine: oldBase + index, newLine: -1, text: before[index] });
    }
    for (let index = 0; index < cols; index += 1) {
      ops.push({ kind: 'insert', oldLine: -1, newLine: newBase + index, text: after[index] });
    }
    return ops;
  }

  const width = cols + 1;
  const table = new Int32Array((rows + 1) * width);
  for (let row = rows - 1; row >= 0; row -= 1) {
    for (let col = cols - 1; col >= 0; col -= 1) {
      table[row * width + col] =
        before[row] === after[col]
          ? table[(row + 1) * width + col + 1] + 1
          : Math.max(table[(row + 1) * width + col], table[row * width + col + 1]);
    }
  }

  const ops: DiffOp[] = [];
  let row = 0;
  let col = 0;
  while (row < rows && col < cols) {
    if (before[row] === after[col]) {
      ops.push({ kind: 'equal', oldLine: oldBase + row, newLine: newBase + col, text: before[row] });
      row += 1;
      col += 1;
    } else if (table[(row + 1) * width + col] >= table[row * width + col + 1]) {
      // Deletions before insertions on a tie, matching `diff`'s own ordering
      // inside a change block.
      ops.push({ kind: 'delete', oldLine: oldBase + row, newLine: -1, text: before[row] });
      row += 1;
    } else {
      ops.push({ kind: 'insert', oldLine: -1, newLine: newBase + col, text: after[col] });
      col += 1;
    }
  }
  while (row < rows) {
    ops.push({ kind: 'delete', oldLine: oldBase + row, newLine: -1, text: before[row] });
    row += 1;
  }
  while (col < cols) {
    ops.push({ kind: 'insert', oldLine: -1, newLine: newBase + col, text: after[col] });
    col += 1;
  }
  return ops;
}

/**
 * Moves each run of changes as far down the file as it can go without changing
 * what the diff means.
 *
 * With repeated lines — and a config file is full of `[[route]]` and
 * `enabled = true` — several minimal scripts exist, and this is the one `diff`
 * prints: a route added in the middle reads as one new block rather than a
 * rename of the block after it.
 */
function slideChangesDown(ops: DiffOp[]): DiffOp[] {
  const result = ops.map((op) => ({ ...op }));
  let index = 0;

  while (index < result.length) {
    if (result[index].kind === 'equal') {
      index += 1;
      continue;
    }

    let end = index;
    while (end < result.length && result[end].kind !== 'equal') end += 1;

    // Only a block that is all insertions or all deletions can slide: with
    // both, moving one side past a line would change what lines up with what.
    const homogeneous = result.slice(index, end).every((op) => op.kind === result[index].kind);
    if (homogeneous) {
      // Sliding is a relabelling, not a move: the first line of the block
      // becomes unchanged and the identical line after it becomes the change.
      // The file content is untouched, which is what makes it safe.
      while (
        end < result.length &&
        result[end].kind === 'equal' &&
        result[end].text === result[index].text
      ) {
        result[end].kind = result[index].kind;
        result[index].kind = 'equal';
        index += 1;
        end += 1;
      }
    }

    index = end;
  }

  return renumber(pairWithNeighbours(result));
}

/**
 * Pulls a block back up when that makes it sit against another change.
 *
 * A line that was replaced should read as a replacement — the removed line and
 * the added one together — and two edits a line apart should read as one block,
 * not two. `diff` joins changes the same way, which is also what keeps the two
 * outputs identical.
 */
function pairWithNeighbours(ops: DiffOp[]): DiffOp[] {
  const result = ops.map((op) => ({ ...op }));
  let index = 0;

  while (index < result.length) {
    if (result[index].kind === 'equal') {
      index += 1;
      continue;
    }

    let end = index;
    while (end < result.length && result[end].kind !== 'equal') end += 1;

    const kind = result[index].kind;
    if (result.slice(index, end).every((op) => op.kind === kind)) {
      // How far up the block could slide: past every unchanged line that
      // repeats the block's own last line.
      let steps = 0;
      while (
        index - steps - 1 >= 0 &&
        result[index - steps - 1].kind === 'equal' &&
        result[index - steps - 1].text === result[end - steps - 1].text
      ) {
        steps += 1;
      }

      const landing = index - steps - 1;
      // Only move when it lands against another change; otherwise leave the
      // block where the downward slide put it.
      if (steps > 0 && landing >= 0 && result[landing].kind !== 'equal') {
        for (let step = 0; step < steps; step += 1) {
          result[index - step - 1].kind = kind;
          result[end - step - 1].kind = 'equal';
        }
        index = end - steps;
        continue;
      }
    }

    index = end;
  }

  return result;
}

/** Reassigns line numbers after ops have been relabelled. */
function renumber(ops: DiffOp[]): DiffOp[] {
  let oldLine = 0;
  let newLine = 0;
  return ops.map((op) => {
    if (op.kind === 'equal') {
      const next = { ...op, oldLine, newLine };
      oldLine += 1;
      newLine += 1;
      return next;
    }
    if (op.kind === 'delete') {
      const next = { ...op, oldLine, newLine: -1 };
      oldLine += 1;
      return next;
    }
    const next = { ...op, oldLine: -1, newLine };
    newLine += 1;
    return next;
  });
}

export function diffStats(ops: DiffOp[]): DiffStats {
  let added = 0;
  let removed = 0;
  for (const op of ops) {
    if (op.kind === 'insert') added += 1;
    else if (op.kind === 'delete') removed += 1;
  }
  return { added, removed, same: added === 0 && removed === 0 };
}

// ---------------------------------------------------------------------------
// Unified view
// ---------------------------------------------------------------------------

export interface UnifiedHunk {
  /** 1-based start line in the old text, and how many lines it covers. */
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  ops: DiffOp[];
}

/** Groups a diff into hunks with `context` unchanged lines around each change. */
export function toHunks(ops: DiffOp[], context = 3): UnifiedHunk[] {
  const changed = ops
    .map((op, index) => (op.kind === 'equal' ? -1 : index))
    .filter((index) => index >= 0);
  if (changed.length === 0) return [];

  const ranges: { start: number; end: number }[] = [];
  for (const index of changed) {
    const start = Math.max(0, index - context);
    const end = Math.min(ops.length - 1, index + context);
    const last = ranges[ranges.length - 1];
    // Ranges that touch or overlap become one hunk, same as `diff -u`.
    if (last && start <= last.end + 1) last.end = Math.max(last.end, end);
    else ranges.push({ start, end });
  }

  // Lines consumed before each op, which is what the @@ header counts from.
  const oldBefore: number[] = [];
  const newBefore: number[] = [];
  let oldSeen = 0;
  let newSeen = 0;
  for (const op of ops) {
    oldBefore.push(oldSeen);
    newBefore.push(newSeen);
    if (op.kind !== 'insert') oldSeen += 1;
    if (op.kind !== 'delete') newSeen += 1;
  }

  return ranges.map(({ start, end }) => {
    const slice = ops.slice(start, end + 1);
    const oldCount = slice.filter((op) => op.kind !== 'insert').length;
    const newCount = slice.filter((op) => op.kind !== 'delete').length;
    return {
      // An empty side is numbered by the line it would follow, which is what
      // `diff` prints for a hunk that only adds or only removes.
      oldStart: oldCount === 0 ? oldBefore[start] : oldBefore[start] + 1,
      oldCount,
      newStart: newCount === 0 ? newBefore[start] : newBefore[start] + 1,
      newCount,
      ops: slice
    };
  });
}

/** The hunks rendered the way `diff -u` prints them, headers included. */
export function toUnifiedText(ops: DiffOp[], context = 3): string {
  const lines: string[] = [];
  for (const hunk of toHunks(ops, context)) {
    lines.push(
      `@@ -${range(hunk.oldStart, hunk.oldCount)} +${range(hunk.newStart, hunk.newCount)} @@`
    );
    for (const op of hunk.ops) {
      const marker = op.kind === 'insert' ? '+' : op.kind === 'delete' ? '-' : ' ';
      lines.push(`${marker}${op.text}`);
      // `diff` says so right under the line it is missing from.
      if (op.noNewline) lines.push('\\ No newline at end of file');
    }
  }
  return lines.length > 0 ? `${lines.join('\n')}\n` : '';
}

function range(start: number, count: number): string {
  return count === 1 ? `${start}` : `${start},${count}`;
}

// ---------------------------------------------------------------------------
// Side-by-side view
// ---------------------------------------------------------------------------

export interface SideBySideRow {
  kind: 'equal' | 'replace' | 'insert' | 'delete';
  left: { line: number; text: string } | null;
  right: { line: number; text: string } | null;
}

/**
 * The same diff as two aligned columns.
 *
 * Deletions and insertions inside one change block are paired up so a changed
 * line sits opposite the line it replaced, rather than the two drifting apart.
 */
export function toSideBySide(ops: DiffOp[]): SideBySideRow[] {
  const rows: SideBySideRow[] = [];
  let index = 0;

  while (index < ops.length) {
    const op = ops[index];
    if (op.kind === 'equal') {
      rows.push({
        kind: 'equal',
        left: { line: op.oldLine + 1, text: op.text },
        right: { line: op.newLine + 1, text: op.text }
      });
      index += 1;
      continue;
    }

    const deletes: DiffOp[] = [];
    const inserts: DiffOp[] = [];
    while (index < ops.length && ops[index].kind !== 'equal') {
      if (ops[index].kind === 'delete') deletes.push(ops[index]);
      else inserts.push(ops[index]);
      index += 1;
    }

    const pairs = Math.max(deletes.length, inserts.length);
    for (let pair = 0; pair < pairs; pair += 1) {
      const left = deletes[pair];
      const right = inserts[pair];
      rows.push({
        kind: left && right ? 'replace' : left ? 'delete' : 'insert',
        left: left ? { line: left.oldLine + 1, text: left.text } : null,
        right: right ? { line: right.newLine + 1, text: right.text } : null
      });
    }
  }

  return rows;
}

/**
 * Diffs two whole texts.
 *
 * A file that lost its trailing newline differs from one that has it, and
 * `diff` says so, so the last line of such a file is marked and compares
 * unequal to the same line with a newline after it.
 */
export function diffText(before: string, after: string): DiffOp[] {
  const beforeLines = splitLines(before);
  const afterLines = splitLines(after);
  const markBefore = before.length > 0 && !before.endsWith('\n');
  const markAfter = after.length > 0 && !after.endsWith('\n');
  if (markBefore) beforeLines[beforeLines.length - 1] += NO_NEWLINE;
  if (markAfter) afterLines[afterLines.length - 1] += NO_NEWLINE;

  return diffLines(beforeLines, afterLines).map((op) =>
    op.text.endsWith(NO_NEWLINE)
      ? { ...op, text: op.text.slice(0, -NO_NEWLINE.length), noNewline: true }
      : op
  );
}

/** NUL cannot appear in a TOML file, so it cannot collide with real text. */
const NO_NEWLINE = '\u0000';

/** Two whole texts, rendered exactly as `diff -u` would print the hunks. */
export function unifiedDiff(before: string, after: string, context = 3): string {
  return toUnifiedText(diffText(before, after), context);
}
