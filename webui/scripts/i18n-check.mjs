// Keeps the two languages in step (T309 acceptance).
//
// Three things it will not let through:
//
//   1. a key in one dictionary and not the other — the reason a translated UI
//      falls back to English in three places nobody notices until a user does;
//   2. a key nothing uses, or a `t('…')` for a key that does not exist;
//   3. a string rendered from a component without going through `t()` at all,
//      which is the way an untranslated UI grows back after it is translated.
//
// The third is a heuristic over the templates. What it cannot see is listed in
// ALLOWED below, with a reason each — the list is short on purpose.
//
// Usage: npm run i18n:check [--verbose]
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';

const ROOT = new URL('../src/', import.meta.url).pathname;
const verbose = process.argv.includes('--verbose');

const failures = [];
const fail = (what) => failures.push(what);

// --- the dictionaries -------------------------------------------------------

/** Parses `'key': 'value',` out of a dictionary without importing TypeScript. */
async function readDictionary(file) {
  const source = await readFile(join(ROOT, 'lib/i18n', file), 'utf8');
  const entries = new Map();
  const pattern = /^\s*'([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")\s*,?\s*$/gm;
  let match;
  while ((match = pattern.exec(source)) !== null) {
    const [, key, single, double] = match;
    if (entries.has(key)) fail(`${file}: duplicate key ${key}`);
    entries.set(key, single ?? double ?? '');
  }
  return entries;
}

const english = await readDictionary('en.ts');
const thai = await readDictionary('th.ts');

if (english.size === 0) fail('en.ts has no keys — did the parser break?');

for (const key of english.keys()) {
  if (!thai.has(key)) fail(`missing in th.ts: ${key}`);
}
for (const key of thai.keys()) {
  if (!english.has(key)) fail(`missing in en.ts: ${key}`);
}

// A sentence that is identical in both languages is usually a missed
// translation. A word or two is usually a term of art — Route, Host, TOML,
// HTTP/2 — which Thai infrastructure work uses in English as well, so only
// strings of three words or more are held to this.
const SAME_IN_BOTH = new Set([
  "tls.acme.production", // a brand name, either way
  'tls.acme.staging'
]);

/** Words, ignoring placeholders and punctuation. */
const wordCount = (text) =>
  text
    .replace(/\{\w+\}/g, '')
    .split(/[\s—·]+/)
    .filter((word) => /[A-Za-z]{2,}/.test(word)).length;

for (const [key, value] of english) {
  const translated = thai.get(key);
  if (translated === undefined) continue;
  const bothAscii = /^[\x20-\x7e]*$/.test(translated);
  if (
    translated === value &&
    bothAscii &&
    !SAME_IN_BOTH.has(key) &&
    wordCount(value) >= 3 &&
    /[A-Za-z]{4,}/.test(value)
  ) {
    fail(`th.ts still holds the English string for ${key}: "${value}"`);
  }
  // Placeholders have to survive translation, or the sentence loses its number.
  const placeholders = (text) => (text.match(/\{(\w+)\}/g) ?? []).sort().join(',');
  if (placeholders(value) !== placeholders(translated)) {
    fail(`placeholders differ for ${key}: en ${placeholders(value) || 'none'} vs th ${placeholders(translated) || 'none'}`);
  }
}

// Counted strings come in pairs.
for (const key of english.keys()) {
  if (key.endsWith('.one') && !english.has(`${key.slice(0, -4)}.other`)) {
    fail(`${key} has no .other form`);
  }
  if (key.endsWith('.other') && !english.has(`${key.slice(0, -6)}.one`)) {
    fail(`${key} has no .one form`);
  }
}

// --- what the source uses ---------------------------------------------------

/**
 * Keys reached through a variable rather than by name — `$t(`status.${x}`)` —
 * so "used" cannot be seen by searching for the literal.
 */
const DYNAMIC = [
  'nav.',
  'connection.',
  'status.',
  'shell.theme.',
  'settings.tab.',
  'observability.level.',
  'admin.planned.',
  'conflict.tab.',
  'dashboard.live.',
  'dashboard.upstreams.reason.',
  'routes.column.',
  'routes.toast.enabled',
  'routes.toast.disabled',
  'routeTester.matched.',
  // Template copy is reached through `titleKey` / `descriptionKey`.
  'template.single.',
  'template.blueGreen.',
  'template.gateway.'
];

async function walk(directory) {
  const out = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(path)));
    else if (/\.(svelte|ts)$/.test(entry.name)) out.push(path);
  }
  return out;
}

const files = (await walk(ROOT)).filter((path) => !path.includes('/lib/i18n/'));
const used = new Set();

for (const path of files) {
  const source = await readFile(path, 'utf8');
  // `(?<![\w.$])` so that `set('server.listen')` and `fetch('/x')` are not
  // mistaken for a translation call ending in t.
  for (const match of source.matchAll(/(?<![\w.$])\$?(?:t|tr)\(\s*'([^']+)'/g)) used.add(match[1]);
  for (const match of source.matchAll(/(?<![\w.$])\$?plural\(\s*'([^']+)'/g)) {
    used.add(`${match[1]}.one`);
    used.add(`${match[1]}.other`);
  }
}

for (const key of used) {
  if (!english.has(key)) fail(`used but not defined: ${key}`);
}
for (const key of english.keys()) {
  // A key built at runtime (`nav.badge.*`, severity names) is used through a
  // variable, so "unused" here means "unused by name".
  if (!used.has(key) && !DYNAMIC.some((prefix) => key.startsWith(prefix))) {
    fail(`defined but never used: ${key}`);
  }
}

// --- strings that never made it into a dictionary ---------------------------

/** Files whose strings are not shown to a user of the proxy. */
const SKIPPED = [
  '/lib/components/ui/', // primitives: no copy of their own
  '/lib/styleguide/', // a page for whoever is building the UI
  '/lib/i18n/'
];

/**
 * Text the check cannot judge, each for a reason:
 * - names of things: the product, the format, the config keys themselves;
 * - symbols and units that are the same in both languages.
 */
const ALLOWED = new Set([
  'prx',
  'PRX',
  'P',
  'TOML',
  'JSON',
  'ACME',
  'TLS',
  'SNI',
  'HTTP/2',
  'h2c',
  'p50',
  'p95',
  'p99',
  '⌘K',
  'Ctrl',
  'F',
  '·',
  '—',
  '@@',
  '+',
  '−',
  '-',
  'req/s',
  'ms',
  '%',
  '\\ No newline at end of file'
]);

const visibleAttributes = /\b(aria-label|placeholder|title|alt|label|description|heading|confirmLabel|cancelLabel|hint)=(?:"([^"]*)"|'([^']*)')/g;

/**
 * An example value rather than a sentence: a path, a host, an address, an
 * email, a config key, or a comma-separated list of those. Translating
 * `./certs/tls.crt` would be a mistake, not an improvement.
 */
const isSampleValue = (text) =>
  /^[\w./*:@-]+(\[\])?(,\s*[\w./*:@-]+)*$/.test(text) && !/\s(a|the|to|of|is)\s/.test(text);

for (const path of files) {
  if (SKIPPED.some((skip) => path.includes(skip))) continue;
  if (!path.endsWith('.svelte')) continue;

  const source = await readFile(path, 'utf8');
  const relative = path.slice(ROOT.length);

  // Templates only: the script block is checked by the rules below it.
  const template = source
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '');

  // Text between tags, with expressions taken out first.
  const withoutExpressions = template.replace(/\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}/g, '');
  for (const match of withoutExpressions.matchAll(/>([^<>]+)</g)) {
    const text = match[1].replace(/\s+/g, ' ').trim();
    if (!text || ALLOWED.has(text) || isSampleValue(text)) continue;
    // Two letters in a row is a word; `+1`, `12`, `:` and friends are not.
    if (!/[A-Za-z]{2,}/.test(text)) continue;
    fail(`${relative}: untranslated text “${text}”`);
  }

  for (const match of template.matchAll(visibleAttributes)) {
    const [, attribute, doubleQuoted, singleQuoted] = match;
    const text = (doubleQuoted ?? singleQuoted ?? '').trim();
    if (!text || ALLOWED.has(text) || isSampleValue(text)) continue;
    if (!/[A-Za-z]{2,}/.test(text)) continue;
    fail(`${relative}: untranslated ${attribute} “${text}”`);
  }

  // Anything shown as a toast is a sentence a person reads.
  for (const match of source.matchAll(/toast\.\w+\(\s*(['"`])([^'"`]{4,})\1/g)) {
    fail(`${relative}: untranslated toast “${match[2]}”`);
  }
}

if (verbose && failures.length === 0) {
  console.log(`${english.size} keys, both languages, ${used.size} used by name`);
}

if (failures.length > 0) {
  console.error(`i18n check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(
  `i18n check passed — ${english.size} keys in en and th, no untranslated strings in the pages`
);
