// Fails the build when the bundle outgrows its budget.
//
// The UI is embedded in the prx binary with include_dir!, so every kilobyte
// here is a kilobyte in the shipped proxy. A budget that fails the build is
// the only kind anyone notices.
import { readdir, stat, readFile } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';
import { join } from 'node:path';

const BUDGETS = {
  // Everything the first paint needs.
  js: 250 * 1024, // gzipped
  // The config editor (T307), fetched only when the TOML tab is opened. It is
  // CodeMirror, so it is big; keeping it out of `js` is what makes that
  // acceptable, and this ceiling is what keeps it from growing further.
  editorJs: 150 * 1024, // gzipped
  css: 60 * 1024, // gzipped
  fonts: 200 * 1024 // raw; woff2 is already compressed
};

const DIST = new URL('../dist/', import.meta.url).pathname;

async function walk(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(path)));
    else out.push(path);
  }
  return out;
}

const files = await walk(DIST);
const totals = { js: 0, editorJs: 0, css: 0, fonts: 0 };

/** The lazily loaded editor chunks: the CodeMirror bundle and its setup. */
const isEditorChunk = (path) => /\/editor(Setup)?-[^/]*\.js$/.test(path);

for (const path of files) {
  if (path.endsWith('.js')) {
    const size = gzipSync(await readFile(path)).length;
    if (isEditorChunk(path)) totals.editorJs += size;
    else totals.js += size;
  } else if (path.endsWith('.css')) totals.css += gzipSync(await readFile(path)).length;
  else if (/\.(woff2?|ttf|otf)$/.test(path)) totals.fonts += (await stat(path)).size;
}

if (totals.editorJs === 0) {
  console.error('no editor chunk in the build: the config editor is no longer code split');
  process.exit(1);
}

const kb = (n) => `${(n / 1024).toFixed(1)} KB`;
const failures = [];
for (const [kind, used] of Object.entries(totals)) {
  const budget = BUDGETS[kind];
  const label =
    kind === 'fonts'
      ? 'fonts (raw)'
      : kind === 'editorJs'
        ? 'editor chunk, lazy (gzip)'
        : `${kind} (gzip)`;
  const line = `${label}: ${kb(used)} / ${kb(budget)}`;
  if (used > budget) failures.push(line);
  else console.log(`  ok  ${line}`);
}

if (failures.length) {
  console.error('\nbundle budget exceeded:');
  for (const f of failures) console.error(`  FAIL ${f}`);
  console.error('\nTrim the bundle or raise the budget deliberately in scripts/bundle-budget.mjs.');
  process.exit(1);
}
