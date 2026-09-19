// Drives the Settings page against a mocked admin API (T308 acceptance).
//
// What a type checker cannot see here: that a form field edits the file rather
// than re-rendering it, that the edit lands in the same draft the TOML editor
// shows and leaves through the same review, that certificates about to expire
// are impossible to miss, and that the page is honest about the admin API
// having no authentication yet.
//
// Usage: npm run settings:check [--verbose]
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

const PORT = Number(process.env.SETTINGS_PORT ?? 5208);
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

// --- the file the mocked admin API is serving -------------------------------

const ORIGINAL = `# Written by hand, and it should stay that way.
[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[server.tls]
listen = "0.0.0.0:8443"
cert_path = "./certs/tls.crt"
key_path = "./certs/tls.key"
enable_h2 = true

[observability]
log_level = "info"
access_log = true

[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3001"

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
`;

let fileText = ORIGINAL;
const etagFor = (text) => `"len-${text.length}"`;

/**
 * Stands in for `src/config_edit.rs`: enough of it to answer the ops this
 * check makes, by rewriting the one line the op names. The real thing is
 * `toml_edit`, and it has its own tests; what matters here is that the page
 * sends the right op and renders whatever comes back.
 */
function applyOps(toml, ops) {
  let text = toml;
  for (const op of ops) {
    const key = op.path.split('.').pop();
    const literal =
      typeof op.value === 'string'
        ? `"${op.value}"`
        : Array.isArray(op.value)
          ? `[${op.value.map((item) => `"${item}"`).join(', ')}]`
          : String(op.value);

    if (op.action === 'remove' || op.value === null || op.value === undefined) {
      text = text.replace(new RegExp(`^${key} = .*\\n`, 'm'), '');
      continue;
    }

    const line = new RegExp(`^${key} = .*$`, 'm');
    text = line.test(text)
      ? text.replace(line, `${key} = ${literal}`)
      : `${text}${key} = ${literal}\n`;
  }
  return text;
}

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

/** The parsed form of whatever text is passed in, for the fields this drives. */
const configFor = (text) => ({
  server: {
    listen: JSON.parse((/^listen = (\[.*\])$/m.exec(text)?.[1] ?? '[]').replaceAll("'", '"')),
    health_path: /^health_path = "([^"]*)"/m.exec(text)?.[1] ?? '/healthz',
    ready_path: /^ready_path = "([^"]*)"/m.exec(text)?.[1] ?? '/readyz',
    threads: Number(/^threads = (\d+)/m.exec(text)?.[1]) || null,
    grace_period_seconds: null,
    graceful_shutdown_timeout_seconds: null,
    config_reload_debounce_ms: 250,
    h2c: !/^h2c = false/m.test(text),
    tls: /\[server\.tls\]/.test(text)
      ? {
          listen: '0.0.0.0:8443',
          cert_path: './certs/tls.crt',
          key_path: './certs/tls.key',
          enable_h2: true,
          certs: [],
          acme: {
            enabled: /^enabled = true/m.test(text),
            email: [],
            directory_url: 'https://acme-staging-v02.api.letsencrypt.org/directory',
            domains: [],
            storage_dir: '/var/lib/prx/acme',
            renew_before_days: 30,
            ca_root_path: null
          }
        }
      : null
  },
  observability: {
    log_level: /^log_level = "([^"]*)"/m.exec(text)?.[1] ?? 'info',
    access_log: !/^access_log = false/m.test(text),
    prometheus_listen: ''
  },
  services: [
    {
      name: 'api',
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
        kind: 'http',
        path: '/healthz',
        interval_ms: 2000,
        timeout_ms: 1000,
        healthy_threshold: 2,
        unhealthy_threshold: 3,
        expected_status: [200]
      },
      sticky: { enabled: false, mode: 'cookie', name: 'prx_upstream', ttl_s: 3600 },
      upstreams: [upstream('127.0.0.1:3001')]
    }
  ],
  routes: [
    {
      name: 'api',
      service: 'api',
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
    }
  ]
});

const report = (text) => ({
  valid: true,
  errors: [],
  warnings: [],
  config: configFor(text),
  current_etag: etagFor(fileText)
});

const day = 86_400;
const now = Math.floor(Date.now() / 1000);
const tlsStatus = () => ({
  acme: {
    enabled: true,
    staging: true,
    directory_url: 'https://acme-staging-v02.api.letsencrypt.org/directory',
    domains: ['example.com'],
    last_attempt_epoch_s: now - 3600,
    last_success_epoch_s: now - 3600,
    last_error: 'the last order failed: rateLimited',
    certificate_expiry_epoch_s: now + 12 * day
  },
  certificates: [
    { domain: 'example.com', expires_epoch_s: now + 12 * day, expires_in_days: 12 },
    { domain: 'old.example.com', expires_epoch_s: now - 2 * day, expires_in_days: -2 },
    { domain: 'fine.example.com', expires_epoch_s: now + 200 * day, expires_in_days: 200 }
  ]
});

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1500, height: 1200 } });

/** Every edit op the page sent, which is the contract with the server. */
const sentOps = [];
let appliedBody = null;
let renewRequests = 0;

await context.route('**/web/config/validate', async (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify(report(route.request().postData() ?? ''))
  })
);

await context.route('**/web/config/edit', async (route) => {
  const payload = JSON.parse(route.request().postData() ?? '{}');
  sentOps.push(...(payload.ops ?? []));
  const edited = applyOps(payload.toml ?? '', payload.ops ?? []);
  return route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({ toml: edited, ...report(edited) })
  });
});

await context.route('**/web/tls/acme/renew', (route) => {
  renewRequests += 1;
  return route.fulfill({ status: 202, contentType: 'text/plain', body: 'renew_requested\n' });
});

await context.route('**/web/tls/status', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(tlsStatus()) })
);

await context.route('**/web/config*', (route) => {
  const request = route.request();

  if (request.method() === 'HEAD') {
    return route.fulfill({ status: 200, headers: { etag: etagFor(fileText) }, body: '' });
  }
  if (request.method() === 'PUT') {
    appliedBody = request.postData();
    fileText = appliedBody ?? fileText;
    return route.fulfill({
      status: 200,
      contentType: 'text/plain',
      headers: { etag: etagFor(fileText) },
      body: 'config_applied\n'
    });
  }
  if (new URL(request.url()).searchParams.get('format') === 'json') {
    return route.fulfill({
      contentType: 'application/json',
      headers: { etag: etagFor(fileText) },
      body: JSON.stringify(configFor(fileText))
    });
  }
  return route.fulfill({
    contentType: 'text/plain; charset=utf-8',
    headers: { etag: etagFor(fileText) },
    body: fileText
  });
});

await context.route('**/web/health/routes*', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({ checked_at_epoch_ms: Date.now(), timeout_ms: 1200, routes: [] })
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
const tab = (name) => page.getByRole('button', { name, exact: true });
const editorText = () =>
  page.evaluate(() => {
    const lines = document.querySelectorAll('[data-slot="toml-editor"] .cm-line');
    return [...lines].map((line) => line.textContent).join('\n');
  });

await page.goto(`${ORIGIN}/settings`, { waitUntil: 'networkidle' });
await settle(700);

// --- 1. the fields are there, and say what they cost -------------------------

check((await page.locator('[data-slot="setting-row"]').count()) >= 6, 'the server tab has fields');
check(
  (await page.locator('[data-slot="setting-row"][data-restart="true"]').count()) >= 6,
  'the settings prx only reads at startup are marked as needing a restart'
);
check(
  (await page.locator('text=needs a restart').first().isVisible()) === true,
  'and the mark says so in words, not only in colour'
);

// --- 2. a form field edits the file, key by key ------------------------------

await tab('Observability').click();
await settle(300);
await page.getByLabel('Prometheus listener').fill('127.0.0.1:9100');
await page.getByLabel('Prometheus listener').blur();
await settle(700);

const listenOp = sentOps.find((op) => op.path === 'observability.prometheus_listen');
check(Boolean(listenOp), 'changing a field sends an edit op for that key');
check(listenOp?.value === '127.0.0.1:9100', 'with the value that was typed');
check(
  sentOps.every((op) => typeof op.path === 'string' && op.path.length > 0),
  'every op names the key it changes'
);

const dirty = await page.locator('[data-slot="draft-stats"]').innerText();
check(/\+1/.test(dirty), `the draft bar counts the change (${dirty.replace(/\s+/g, ' ')})`);

// --- 3. the same draft the editor shows, comments and all --------------------

await tab('TOML').click();
await page.waitForSelector('[data-slot="toml-editor"] .cm-line');
await settle(600);
const draftText = await editorText();
check(
  draftText.includes('prometheus_listen = "127.0.0.1:9100"'),
  'the editor shows the field the form just changed'
);
check(
  draftText.includes('# Written by hand, and it should stay that way.'),
  'and the comment at the top of the file is still there'
);

// --- 4. it leaves through the same review and apply --------------------------

await page.getByRole('button', { name: /Review & apply/ }).click();
await page.waitForSelector('[aria-label="Review config changes"]');
const review = page.locator('[aria-label="Review config changes"]');
check((await review.innerText()).includes('@@'), 'the review shows the diff of the file');
check(
  (await review.innerText()).includes('prometheus_listen'),
  'and names the field that changed'
);

await review.getByRole('button', { name: /Apply to the proxy/ }).click();
await page.waitForSelector('text=Config applied');
check(
  (appliedBody ?? '').includes('prometheus_listen = "127.0.0.1:9100"'),
  'applying writes the edited file'
);
check(
  (appliedBody ?? '').includes('# Written by hand'),
  'and the file that reaches the proxy still has its comments'
);

// --- 5. certificates ---------------------------------------------------------

await tab('TLS').click();
await page.waitForSelector('[data-slot="tls-status"]');
await settle(500);

const status = page.locator('[data-slot="tls-status"]');
check((await status.innerText()).includes('example.com'), 'the TLS tab lists what is served');
check(
  (await status.innerText()).includes('12 days left'),
  'a certificate close to expiry says how long it has'
);
check(
  (await status.innerText()).includes('expired 2 days ago'),
  'and an expired one says it is expired'
);
check(
  (await status.locator('.bg-warning').count()) >= 1 &&
    (await status.locator('.bg-destructive').count()) >= 1,
  'both are badged, not only coloured text'
);

check(
  (await page.locator('[data-slot="acme"]').count()) === 1,
  'ACME is configurable from here'
);
await page.getByRole('button', { name: 'Order a certificate now' }).click();
await settle(400);
check(renewRequests === 1, 'ordering a certificate reaches the admin API');
check(
  (await page.locator('body').innerText()).includes('rateLimited'),
  'and the last failure is shown rather than swallowed'
);

// Turning TLS off is destructive enough to ask first.
await page.getByLabel('TLS listener').click();
await settle(400);
check(
  (await page.locator('[role="dialog"]', { hasText: 'Remove the TLS listener?' }).count()) === 1,
  'removing the TLS listener asks for confirmation'
);
await page.getByRole('button', { name: 'Cancel' }).first().click();
await settle(300);

// --- 6. the admin tab is honest ---------------------------------------------

await tab('Admin API').click();
await settle(300);
const admin = await page.locator('#main-content').innerText();
check(
  admin.includes('no authentication yet'),
  'the admin tab says the API is unauthenticated instead of showing switches that do nothing'
);
check(admin.includes('T201'), 'and names the task that will fix it');

// --- 7. nothing threw --------------------------------------------------------

check(pageErrors.length === 0, `no uncaught errors (${pageErrors.join(' | ')})`);

await browser.close();
stop();

if (failures.length > 0) {
  console.error(`\nsettings check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  FAIL ${failure}`);
  process.exit(1);
}

console.log('settings check passed');
