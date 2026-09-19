// The parts of T309 that only exist in a browser.
//
// Four claims: that an unconfigured proxy offers a way out of being
// unconfigured, that the three questions produce a config in the draft, that
// switching to Thai changes the text rather than half of it, and that a slow
// admin API shows the shape of the page instead of moving it about once the
// data lands.
//
// Usage: npm run ux:check [--verbose]
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

const PORT = Number(process.env.UX_PORT ?? 5210);
const ORIGIN = `http://127.0.0.1:${PORT}`;

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

// --- a proxy with nothing in it ---------------------------------------------

const EMPTY_TOML = `[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"
`;

let fileText = EMPTY_TOML;

/**
 * The mocked admin API parses just enough of the file to be honest about it.
 *
 * It has to: the wizard offers itself when the config has no services, so a
 * mock that keeps saying "empty" after the wizard has written one would offer
 * again on the next page load — and the check would be testing the mock.
 */
const parseConfig = (toml) => {
  const services = [...toml.matchAll(/\[\[service\]\][^[]*name = "([^"]+)"/g)].map(
    ([, name]) => ({
      name,
      lb: 'round_robin',
      max_retries: 0,
      retry_backoff_ms: 0,
      retry_budget_ratio: 0.2,
      retry_budget_min_per_window: 10,
      retry_budget_window_ms: 10000,
      retry_idempotent_only: true,
      request_timeout_ms: 0,
      upstream_h2: 'never',
      circuit_breaker: { enabled: false, consecutive_failures: 3, open_ms: 30000 },
      health_check: {
        enabled: false,
        kind: 'tcp',
        path: '/healthz',
        interval_ms: 2000,
        timeout_ms: 1000,
        healthy_threshold: 2,
        unhealthy_threshold: 3,
        expected_status: [200]
      },
      sticky: { enabled: false, mode: 'cookie', name: 'prx_upstream', ttl_s: 3600 },
      upstreams: [...toml.matchAll(/\[\[service\.upstream\]\]\naddr = "([^"]+)"/g)].map(
        ([, addr]) => ({
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
        })
      )
    })
  );

  return {
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
    services,
    routes: [...toml.matchAll(/\[\[route\]\]\nname = "([^"]+)"\nservice = "([^"]+)"/g)].map(
      ([, name, service]) => ({
        name,
        service,
        host: '',
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
      })
    )
  };
};

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });

/** Held back on demand, to see what the page does while it waits. */
let configDelayMs = 0;

// Validation reports on the text it was sent, which is the draft, not the file.
await context.route('**/web/config/validate', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({
      valid: true,
      errors: [],
      warnings: [],
      config: parseConfig(route.request().postData() ?? ''),
      current_etag: '"ux-1"'
    })
  })
);
await context.route('**/web/config/edit', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({
      toml: fileText,
      valid: true,
      errors: [],
      warnings: [],
      config: parseConfig(fileText),
      current_etag: '"ux-1"'
    })
  })
);
await context.route('**/web/config*', async (route) => {
  const request = route.request();
  if (configDelayMs > 0) await new Promise((resolve) => setTimeout(resolve, configDelayMs));

  if (request.method() === 'HEAD') {
    return route.fulfill({ status: 200, headers: { etag: '"ux-1"' }, body: '' });
  }
  if (request.method() === 'PUT') {
    fileText = request.postData() ?? fileText;
    return route.fulfill({
      status: 200,
      contentType: 'text/plain',
      headers: { etag: '"ux-2"' },
      body: 'config_applied\n'
    });
  }
  if (new URL(request.url()).searchParams.get('format') === 'json') {
    return route.fulfill({
      contentType: 'application/json',
      headers: { etag: '"ux-1"' },
      body: JSON.stringify(parseConfig(fileText))
    });
  }
  return route.fulfill({
    contentType: 'text/plain; charset=utf-8',
    headers: { etag: '"ux-1"' },
    body: fileText
  });
});
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
await context.route('**/web/tls/status', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({
      acme: {
        enabled: false,
        staging: false,
        directory_url: '',
        domains: [],
        last_attempt_epoch_s: null,
        last_success_epoch_s: null,
        last_error: null,
        certificate_expiry_epoch_s: null
      },
      certificates: []
    })
  })
);
await context.route('**/web/stats*', (route) =>
  route.fulfill({ status: 503, contentType: 'text/plain', body: 'no stats in this check\n' })
);

const page = await context.newPage();
page.setDefaultTimeout(15_000);

const pageErrors = [];
page.on('pageerror', (error) => pageErrors.push(`uncaught: ${error.message}`));
page.on('console', (message) => {
  if (message.type() !== 'error') return;
  if (/Failed to load resource/.test(message.text())) return;
  pageErrors.push(`console: ${message.text()}`);
});

const settle = (ms = 400) => page.waitForTimeout(ms);

// --- 1. an empty proxy offers the way out -----------------------------------

await page.goto(`${ORIGIN}/`, { waitUntil: 'networkidle' });
await settle(900);

check(
  (await page.locator('[data-slot="setup-wizard"]').count()) === 1,
  'a proxy with no services opens the setup wizard by itself'
);
check(
  (await page.locator('[data-slot="template-picker"]').count()) === 1,
  'and offers the templates from the playbook alongside it'
);

// --- 2. three questions, then a diff ----------------------------------------

const started = Date.now();
const wizard = page.locator('[data-slot="setup-wizard"]');

await wizard.getByLabel('Service name').fill('checkout');
await wizard.getByRole('button', { name: 'Next', exact: true }).click();
await settle(200);

await wizard.getByLabel('Upstream 1').fill('10.0.0.1:8080');
await wizard.getByRole('button', { name: 'Add another' }).click();
await wizard.getByLabel('Upstream 2').fill('10.0.0.2:8080');
await wizard.getByRole('button', { name: 'Next', exact: true }).click();
await settle(200);

check(
  (await wizard.innerText()).includes('checkout'),
  'the last step previews the config the answers add up to'
);
check(
  (await wizard.innerText()).includes('10.0.0.2:8080'),
  'including every upstream that was typed'
);

await wizard.getByRole('button', { name: /Put it in the draft/ }).click();
await page.waitForSelector('[aria-label="Review config changes"]');
const elapsed = Date.now() - started;
check(elapsed < 120_000, `the wizard reaches a reviewable config quickly (${elapsed} ms)`);

const review = page.locator('[aria-label="Review config changes"]');
check((await review.innerText()).includes('@@'), 'the wizard leaves through the same diff');
check(
  (await review.innerText()).includes('checkout'),
  'and the diff is the config the wizard wrote'
);

await review.getByRole('button', { name: /Apply to the proxy/ }).click();
await page.waitForSelector('text=Config applied');
check(fileText.includes('name = "checkout"'), 'applying writes the wizard config to the proxy');

// --- 3. Thai --------------------------------------------------------------

await page.getByRole('button', { name: /^Language:/ }).click();
await settle(300);
await page.getByRole('menuitemradio', { name: 'ไทย' }).click();
await settle(500);

check(
  (await page.evaluate(() => document.documentElement.lang)) === 'th',
  'switching language sets the document language, which is what a screen reader follows'
);

const sidebar = await page.locator('[data-slot="sidebar"]').innerText();
check(sidebar.includes('ภาพรวม'), 'the sidebar is in Thai');
check(!sidebar.includes('Dashboard'), 'with no English left in it');

await page.goto(`${ORIGIN}/settings`, { waitUntil: 'networkidle' });
await settle(700);
const settingsText = await page.locator('#main-content').innerText();
check(settingsText.includes('ตั้งค่า'), 'and so is the settings page after a reload');
check(
  !/needs a restart|Review & apply|Listen addresses/.test(settingsText),
  'with none of the English copy left behind'
);

// Back to English for the rest of the run.
await page.getByRole('button', { name: /^ภาษา:/ }).click();
await settle(300);
await page.getByRole('menuitemradio', { name: 'English' }).click();
await settle(400);

// --- 4. a slow admin API shows the shape, then fills it in -------------------

configDelayMs = 1200;
await page.goto(`${ORIGIN}/routes`, { waitUntil: 'commit' });
await page.waitForSelector('[data-slot="skeleton-table"]');
const skeletonBox = await page.locator('[data-slot="skeleton-table"]').boundingBox();
check(Boolean(skeletonBox), 'a slow config shows the table\'s shape rather than a spinner');

await page.waitForSelector('[data-slot="skeleton-table"]', { state: 'detached' });
await settle(400);
const emptyState = await page.locator('main h1').boundingBox();
check(
  Boolean(emptyState) && Math.abs((emptyState?.y ?? 0) - 0) < 200,
  'and the header does not move when the data arrives'
);
configDelayMs = 0;

// --- 5. nothing threw --------------------------------------------------------

check(pageErrors.length === 0, `no uncaught errors (${pageErrors.join(' | ')})`);

await browser.close();
stop();

if (failures.length > 0) {
  console.error(`\nux check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  FAIL ${failure}`);
  process.exit(1);
}

console.log('ux check passed — onboarding, templates, Thai, and loading without a jump');
