// Runs axe-core over every page, in both themes and both languages (T309).
//
// The rule this enforces is the acceptance criterion: no critical or serious
// violation anywhere in the app. Moderate and minor findings are printed, not
// failed on — they are usually a judgement call about landmarks, and failing on
// them would train everyone to ignore the check.
//
// It also covers the two things axe cannot see on its own: that a dialog keeps
// the keyboard inside it, and that the status line which updates by itself is
// announced rather than changing silently.
//
// Usage: npm run a11y:check [--verbose]
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

import { mockDraftEndpoints } from './draft-mocks.mjs';

const PORT = Number(process.env.A11Y_PORT ?? 5209);
const ORIGIN = `http://127.0.0.1:${PORT}`;
const verbose = process.argv.includes('--verbose');

const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const build = spawn('npx', ['vite', 'build'], {
  cwd: new URL('..', import.meta.url).pathname,
  stdio: ['ignore', 'ignore', 'inherit']
});
if ((await new Promise((resolve) => build.on('exit', resolve))) !== 0) {
  console.error('vite build failed');
  process.exit(1);
}

const server = spawn(
  'npx',
  ['vite', 'preview', '--port', String(PORT), '--strictPort', '--host', '127.0.0.1'],
  {
    cwd: new URL('..', import.meta.url).pathname,
    stdio: ['ignore', 'ignore', 'pipe'],
    detached: true
  }
);
const stop = () => {
  try {
    process.kill(-server.pid, 'SIGTERM');
  } catch {
    // Already gone.
  }
};
process.on('exit', stop);

async function waitForServer(timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(ORIGIN)).ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error('vite did not come up in time');
}

// --- what the mocked admin API serves ---------------------------------------

const DRAFT_TOML = `[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "info"
access_log = true
`;

const upstream = (addr) => ({
  addr,
  enabled: true,
  tls: false,
  sni: '',
  weight: 1,
  verify_cert: null,
  verify_hostname: null,
  connect_timeout_ms: null,
  total_connect_timeout_ms: null,
  read_timeout_ms: null,
  write_timeout_ms: null,
  idle_timeout_ms: null
});

const config = () => ({
  server: {
    listen: ['0.0.0.0:8080'],
    health_path: '/healthz',
    ready_path: '/readyz',
    threads: null,
    grace_period_seconds: null,
    graceful_shutdown_timeout_seconds: null,
    config_reload_debounce_ms: 250,
    h2c: true,
    tls: null
  },
  observability: { log_level: 'info', access_log: true, prometheus_listen: '' },
  services: [
    {
      name: 'api',
      lb: 'round_robin',
      max_retries: 1,
      retry_backoff_ms: 50,
      retry_budget_ratio: 0.2,
      retry_budget_min_per_window: 10,
      retry_budget_window_ms: 10000,
      retry_idempotent_only: true,
      request_timeout_ms: 0,
      upstream_h2: 'never',
      circuit_breaker: { enabled: true, consecutive_failures: 3, open_ms: 30000 },
      health_check: {
        enabled: true,
        kind: 'http',
        path: '/healthz',
        interval_ms: 2000,
        timeout_ms: 1000,
        healthy_threshold: 2,
        unhealthy_threshold: 3,
        expected_status: [200]
      },
      sticky: { enabled: false, mode: 'cookie', name: 'prx_upstream', ttl_s: 3600 },
      upstreams: [upstream('127.0.0.1:3001'), upstream('127.0.0.1:3002')]
    }
  ],
  routes: [
    {
      name: 'api',
      service: 'api',
      host: 'api.example.com',
      path_prefix: '/',
      methods: [],
      is_default: true,
      enabled: true,
      request_headers: { set: {}, add: {}, remove: [] },
      response_headers: { set: {}, add: {}, remove: [] },
      rate_limit: {
        enabled: false,
        key: 'client_ip',
        requests_per_second: 100,
        burst: 0,
        response_status: 429,
        retry_after: true,
        entry_ttl_ms: 60000,
        max_entries: 100000
      },
      concurrency_limit: { max_concurrent: 0, response_status: 503 },
      cache: {
        enabled: false,
        ttl_ms: 2000,
        max_body_bytes: 262144,
        max_entries: 10000,
        max_bytes: 134217728,
        cache_status_codes: [200],
        key_query: true,
        vary_headers: [],
        coalesce_wait_ms: 2000,
        add_status_header: true
      }
    }
  ]
});

const failures = [];
const notes = [];

await waitForServer();

const axeSource = await readFile(
  new URL('../node_modules/axe-core/axe.min.js', import.meta.url),
  'utf8'
);

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });

await mockDraftEndpoints(context, { toml: DRAFT_TOML, config });
await context.route('**/web/config*', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(config()) })
);
await context.route('**/web/health/routes*', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({ checked_at_epoch_ms: Date.now(), timeout_ms: 1200, routes: [] })
  })
);
await context.route('**/web/services/status', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({ checked_at_epoch_ms: Date.now(), services: [] })
  })
);
await context.route('**/web/stats*', (route) =>
  route.fulfill({ status: 503, contentType: 'text/plain', body: 'no stats in this check\n' })
);

const page = await context.newPage();
page.setDefaultTimeout(15_000);

/** axe, on whatever is on screen. */
async function audit(where) {
  await page.evaluate(axeSource);
  const results = await page.evaluate(async () => {
    // Colour contrast is measured against the design tokens by
    // `contrast-check.mjs`, which can see both themes at once; axe re-checking
    // it here only duplicates that with a slower renderer.
    const run = await window.axe.run(document, {
      resultTypes: ['violations'],
      rules: { 'color-contrast': { enabled: false } }
    });
    return run.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      help: violation.help,
      // The selector alone is a generated id; the markup says which component
      // it came from.
      nodes: violation.nodes
        .slice(0, 2)
        .map((node) => `${node.target.join(' ')} ${node.html.slice(0, 120)}`)
    }));
  });

  for (const violation of results) {
    const line = `${where}: ${violation.impact} — ${violation.id}: ${violation.help} (${violation.nodes.join(', ')})`;
    if (violation.impact === 'critical' || violation.impact === 'serious') failures.push(line);
    else notes.push(line);
  }
  if (verbose) console.log(`audited ${where}: ${results.length} finding(s)`);
}

// --- every page, in both themes ---------------------------------------------

const PAGES = [
  ['/', 'dashboard'],
  ['/routes', 'routes'],
  ['/services', 'services'],
  ['/tls', 'tls'],
  ['/settings', 'settings'],
  ['/audit', 'audit']
];

for (const theme of ['light', 'dark']) {
  await page.addInitScript((choice) => {
    localStorage.setItem('prx-theme', choice);
    // The wizard opens by itself on an unconfigured proxy; this config has a
    // service, but the dismissal keeps the run deterministic either way.
    localStorage.setItem('prx-wizard-dismissed', '1');
  }, theme);

  for (const [path, name] of PAGES) {
    await page.goto(`${ORIGIN}${path}`, { waitUntil: 'networkidle' });
    await page.waitForTimeout(600);
    await audit(`${name} (${theme})`);
  }
}

// --- Thai, which changes the text but not the structure ---------------------

await page.addInitScript(() => localStorage.setItem('prx-locale', 'th'));
await page.goto(`${ORIGIN}/settings`, { waitUntil: 'networkidle' });
await page.waitForTimeout(600);
await audit('settings (th)');
const htmlLang = await page.evaluate(() => document.documentElement.lang);
if (htmlLang !== 'th') {
  failures.push(`the page still reports lang="${htmlLang}" after switching to Thai`);
}

// --- dialogs: open, audited, and keyboard-trapped ----------------------------

await page.addInitScript(() => localStorage.setItem('prx-locale', 'en'));
await page.goto(`${ORIGIN}/services`, { waitUntil: 'networkidle' });
await page.waitForTimeout(400);
await page.getByRole('button', { name: /Add service/ }).first().click();
await page.waitForSelector('[role="dialog"]');
await audit('service dialog');

// Tabbing through the dialog must not land on the page behind it.
const escaped = await page.evaluate(async () => {
  const dialog = document.querySelector('[role="dialog"]');
  if (!dialog) return 'no dialog';
  return dialog.contains(document.activeElement) ? '' : 'focus started outside the dialog';
});
if (escaped) failures.push(`focus trap: ${escaped}`);

for (let index = 0; index < 25; index += 1) {
  await page.keyboard.press('Tab');
  const outside = await page.evaluate(() => {
    const dialog = document.querySelector('[role="dialog"]');
    return dialog && document.activeElement && !dialog.contains(document.activeElement);
  });
  if (outside) {
    failures.push('focus trap: tabbing left the dialog');
    break;
  }
}
await page.keyboard.press('Escape');
await page.waitForTimeout(400);

// --- the command palette, which is the other keyboard surface ---------------

await page.keyboard.press('Control+k');
await page.waitForTimeout(400);
await audit('command palette');
await page.keyboard.press('Escape');

// --- things that update on their own have to be announced -------------------

await page.goto(`${ORIGIN}/`, { waitUntil: 'networkidle' });
await page.waitForTimeout(500);
const live = await page.evaluate(() => ({
  banner: Boolean(document.querySelector('[data-system-state][aria-live]')),
  regions: document.querySelectorAll('[aria-live]').length
}));
if (!live.banner) {
  failures.push('the system status banner is not an aria-live region');
}
if (verbose) console.log(`aria-live regions on the dashboard: ${live.regions}`);

await browser.close();
stop();

if (notes.length > 0 && verbose) {
  console.log(`\n${notes.length} moderate/minor finding(s):`);
  for (const note of notes) console.log(`  ${note}`);
}

if (failures.length > 0) {
  console.error(`\na11y check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  FAIL ${failure}`);
  process.exit(1);
}

console.log(
  `a11y check passed — axe found no critical or serious violation across ${PAGES.length} pages ` +
    `in both themes, the dialogs and the palette (${notes.length} moderate/minor noted)`
);
