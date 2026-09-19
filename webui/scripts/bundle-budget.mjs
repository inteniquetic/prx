// Fails the build when the bundle outgrows its budget.
//
// The UI is embedded in the prx binary with include_dir!, so every kilobyte
// here is a kilobyte in the shipped proxy. A budget that fails the build is
// the only kind anyone notices.
import { readdir, stat, readFile } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';
import { join } from 'node:path';

const BUDGETS = {
  js: 250 * 1024, // gzipped
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
const totals = { js: 0, css: 0, fonts: 0 };

for (const path of files) {
  if (path.endsWith('.js')) totals.js += gzipSync(await readFile(path)).length;
  else if (path.endsWith('.css')) totals.css += gzipSync(await readFile(path)).length;
  else if (/\.(woff2?|ttf|otf)$/.test(path)) totals.fonts += (await stat(path)).size;
}

const kb = (n) => `${(n / 1024).toFixed(1)} KB`;
const failures = [];
for (const [kind, used] of Object.entries(totals)) {
  const budget = BUDGETS[kind];
  const label = kind === 'fonts' ? 'fonts (raw)' : `${kind} (gzip)`;
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
