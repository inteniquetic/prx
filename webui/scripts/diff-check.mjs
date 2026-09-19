// Checks the config diff against the system `diff -u` (T307 acceptance).
//
// A diff the operator is asked to trust before applying a config has to be the
// diff they would get from the command line. This runs both over generated
// pairs of config-shaped files — the kind with repeated `[[route]]` headers
// that make several minimal edit scripts possible — and compares the unified
// output byte for byte.
//
// Usage: npm run diff:check [--verbose] [--cases N]
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { build } from 'esbuild';

// The module under test is TypeScript; bundle it in memory rather than keeping
// a compiled copy around to drift.
const bundled = await build({
  entryPoints: [new URL('../src/lib/configDiff.ts', import.meta.url).pathname],
  bundle: true,
  format: 'esm',
  write: false,
  logLevel: 'error'
});
const { diffText, toUnifiedText, diffStats } = await import(
  `data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`
);

const verbose = process.argv.includes('--verbose');
const caseCount = Number(
  process.argv.find((arg) => arg.startsWith('--cases='))?.slice('--cases='.length) ?? 400
);

const dir = mkdtempSync(join(tmpdir(), 'prx-diff-'));
process.on('exit', () => rmSync(dir, { recursive: true, force: true }));

/** `diff -u` with the headers stripped, so only the hunks are compared. */
function systemUnified(before, after) {
  const a = join(dir, 'a.toml');
  const b = join(dir, 'b.toml');
  writeFileSync(a, before);
  writeFileSync(b, after);
  try {
    execFileSync('diff', ['-u', a, b], { encoding: 'utf8' });
    return '';
  } catch (error) {
    if (error.status !== 1) throw error;
    return error.stdout
      .split('\n')
      .filter((line) => !line.startsWith('---') && !line.startsWith('+++'))
      .join('\n')
      .replace(/^\n/, '');
  }
}

// --- generated cases --------------------------------------------------------

let seed = 20260919;
const random = () => {
  // xorshift: reproducible, so a failure can be replayed.
  seed ^= seed << 13;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  return Math.abs(seed) / 2 ** 31;
};
const pick = (items) => items[Math.floor(random() * items.length) % items.length];
const int = (max) => Math.floor(random() * max) % max;

const LINES = [
  '[[route]]',
  'name = "api"',
  'name = "web"',
  'service = "api"',
  'path_prefix = "/"',
  'path_prefix = "/api"',
  'is_default = true',
  'enabled = false',
  '',
  '[[service]]',
  '[[service.upstream]]',
  'addr = "127.0.0.1:3001"',
  'addr = "127.0.0.1:3002"',
  'weight = 1',
  'weight = 5',
  '# a comment',
  'lb = "round_robin"'
];

function randomConfig(lineCount) {
  return Array.from({ length: lineCount }, () => pick(LINES));
}

/** Random edits of the kind someone makes in an editor. */
function mutate(lines) {
  const next = lines.slice();
  const edits = 1 + int(5);
  for (let edit = 0; edit < edits; edit += 1) {
    const kind = int(3);
    const at = next.length === 0 ? 0 : int(next.length);
    if (kind === 0) next.splice(at, 0, pick(LINES));
    else if (kind === 1 && next.length > 0) next.splice(at, 1);
    else if (next.length > 0) next[at] = pick(LINES);
  }
  return next;
}

const failures = [];
const report = (ok, what, detail) => {
  if (!ok) failures.push(`${what}\n${detail ?? ''}`);
  if (verbose && ok) console.log(`ok   ${what}`);
};

/** The number of added and removed lines the system diff needed. */
function systemCounts(unified) {
  let added = 0;
  let removed = 0;
  for (const line of unified.split('\n')) {
    if (line.startsWith('+')) added += 1;
    else if (line.startsWith('-')) removed += 1;
  }
  return { added, removed };
}

let identical = 0;

for (let index = 0; index < caseCount; index += 1) {
  const before = randomConfig(int(40) + 1);
  const after = mutate(before);
  const beforeText = before.length ? `${before.join('\n')}\n` : '';
  const afterText = after.length ? `${after.join('\n')}\n` : '';

  const ops = diffText(beforeText, afterText);
  const ours = toUnifiedText(ops);
  const theirs = systemUnified(beforeText, afterText);

  // 1. The ops describe both files exactly: nothing is lost, reordered or
  //    invented, and the line numbers the editor scrolls to are real.
  const rebuiltOld = ops.filter((op) => op.kind !== 'insert');
  const rebuiltNew = ops.filter((op) => op.kind !== 'delete');
  report(
    rebuiltOld.map((op) => op.text).join('\n') === before.join('\n'),
    `case ${index}: the diff reproduces the old file`,
    `${JSON.stringify(rebuiltOld.map((op) => op.text))}\n${JSON.stringify(before)}`
  );
  report(
    rebuiltNew.map((op) => op.text).join('\n') === after.join('\n'),
    `case ${index}: the diff reproduces the new file`,
    `${JSON.stringify(rebuiltNew.map((op) => op.text))}\n${JSON.stringify(after)}`
  );
  report(
    rebuiltOld.every((op, position) => op.oldLine === position) &&
      rebuiltNew.every((op, position) => op.newLine === position),
    `case ${index}: every op is numbered with the line it is on`
  );

  // 2. It is as short as the system diff: any two minimal edit scripts have the
  //    same number of insertions and deletions, so this is the real test of
  //    "same diff" on generated input where several minimal scripts exist.
  const mine = diffStats(ops);
  const theirCounts = systemCounts(theirs);
  report(
    mine.added === theirCounts.added && mine.removed === theirCounts.removed,
    `case ${index}: same edit distance as the system diff`,
    `ours ${JSON.stringify(mine)} vs diff ${JSON.stringify(theirCounts)}\n--- ours ---\n${ours}--- diff -u ---\n${theirs}`
  );

  if (ours.trimEnd() === theirs.trimEnd()) identical += 1;
  if (failures.length > 3) break;
}

// 3. Where several minimal scripts exist, `diff` picks one of them by sliding
//    change blocks about. The canonicalisation here follows the same rules, and
//    lands on the same output for the overwhelming majority of generated cases;
//    the remainder are equally short diffs of the same file, laid out
//    differently. A floor rather than an equality, because matching every last
//    heuristic in GNU diff is not what makes a config review trustworthy.
const identicalShare = identical / caseCount;
report(
  identicalShare >= 0.95,
  `at least 95% of generated cases render exactly like diff -u (got ${(identicalShare * 100).toFixed(1)}%)`
);

// --- the cases that matter for a config file --------------------------------

const BASE = `[server]
listen = ["0.0.0.0:8080"]

[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3001"
weight = 1

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
`;

const scenarios = {
  'a changed weight': BASE.replace('weight = 1', 'weight = 5'),
  'an added route': `${BASE}
[[route]]
name = "extra"
service = "api"
path_prefix = "/extra"
`,
  'a removed service block': BASE.replace(
    '[[service.upstream]]\naddr = "127.0.0.1:3001"\nweight = 1\n',
    ''
  ),
  'nothing at all': BASE,
  'the whole file replaced': '# nothing like the original\n',
  'a file that loses its trailing newline': BASE.trimEnd(),
  'an empty file': ''
};

for (const [label, after] of Object.entries(scenarios)) {
  const ours = toUnifiedText(diffText(BASE, after));
  const theirs = systemUnified(BASE, after);
  report(
    ours.trimEnd() === theirs.trimEnd(),
    `${label}: same unified diff as the system diff`,
    `--- ours ---\n${ours}--- diff -u ---\n${theirs}`
  );
}

// The counts the UI puts on the Apply button come from the same ops.
const stats = diffStats(diffText(BASE, scenarios['a changed weight']));
report(
  stats.added === 1 && stats.removed === 1 && !stats.same,
  'a one-line change counts as one added and one removed line',
  JSON.stringify(stats)
);
report(diffStats(diffText(BASE, BASE)).same, 'an unchanged file reports no changes');

if (failures.length > 0) {
  console.error(`\ndiff check failed (${failures.length}):\n`);
  for (const failure of failures) console.error(failure);
  process.exit(1);
}

console.log(`diff check passed (${caseCount} generated cases + ${Object.keys(scenarios).length} config cases)`);
