// Drives the Services page against a mocked admin API.
//
// The page claims things a type checker cannot see: that health state moves
// without a reload, that the share next to a weight slider is the share the
// balancer uses, that editing a service keeps the settings the form was not
// showing, and that a service still carrying routes cannot be deleted.
//
// Usage: npm run services:check
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

import { mockDraftEndpoints } from './draft-mocks.mjs';

const DRAFT_TOML = `[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "info"
access_log = true
`;


const PORT = Number(process.env.SERVICES_PORT ?? 5205);
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

// --- the config the mocked admin API serves --------------------------------

const upstream = (addr, extra = {}) => ({
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
  idle_timeout_ms: null,
  ...extra
});

let services = [
  {
    name: 'checkout',
    lb: 'round_robin',
    max_retries: 2,
    retry_backoff_ms: 50,
    retry_budget_ratio: 0.2,
    retry_budget_min_per_window: 10,
    retry_budget_window_ms: 10000,
    retry_idempotent_only: true,
    request_timeout_ms: 0,
    upstream_h2: 'never',
    circuit_breaker: { enabled: true, consecutive_failures: 5, open_ms: 5000 },
    // Settings the form only shows on other tabs — an edit must not drop them.
    health_check: {
      enabled: true,
      kind: 'http',
      path: '/healthz',
      interval_ms: 2000,
      timeout_ms: 1000,
      healthy_threshold: 2,
      unhealthy_threshold: 3,
      expected_status: [200, 204]
    },
    sticky: { enabled: true, mode: 'cookie', name: 'prx_upstream', ttl_s: 1800 },
    upstreams: [upstream('10.0.0.1:8080', { weight: 3 }), upstream('10.0.0.2:8080')]
  },
  {
    name: 'search',
    lb: 'p2c_ewma',
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
    upstreams: [upstream('10.0.1.1:8080')]
  }
];

const routes = [
  {
    name: 'checkout-web',
    service: 'checkout',
    host: 'shop.example.com',
    path_prefix: '/checkout',
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
      vary_headers: ['accept-encoding'],
      coalesce_wait_ms: 2000,
      add_status_header: true
    }
  }
];

const config = () => ({
  server: {
    listen: ['0.0.0.0:8080'],
    health_path: '/healthz',
    ready_path: '/readyz',
    threads: null,
    grace_period_seconds: null,
    graceful_shutdown_timeout_seconds: null,
    config_reload_debounce_ms: 300,
    tls: null
  },
  observability: { log_level: 'info', access_log: true, prometheus_listen: '127.0.0.1:9100' },
  services,
  routes
});

/** Live state, mutated during the run so the page has something to notice. */
let checkoutSecondUpstream = {
  addr: '10.0.0.2:8080',
  enabled: true,
  weight: 1,
  share: 0.25,
  available: true,
  circuit_open: false,
  circuit_reopens_in_ms: null,
  consecutive_failures: 0,
  probe_healthy: true,
  last_probe_ms_ago: 800,
  inflight: 2,
  ewma_us: 9000
};

const status = () => ({
  checked_at_epoch_ms: Date.now(),
  services: [
    {
      name: 'checkout',
      lb: 'round_robin',
      health_check_enabled: true,
      circuit_breaker_enabled: true,
      upstreams: [
        {
          addr: '10.0.0.1:8080',
          enabled: true,
          weight: 3,
          share: 0.75,
          available: true,
          circuit_open: false,
          circuit_reopens_in_ms: null,
          consecutive_failures: 0,
          probe_healthy: true,
          last_probe_ms_ago: 700,
          inflight: 5,
          ewma_us: 8200
        },
        checkoutSecondUpstream
      ]
    },
    {
      name: 'search',
      lb: 'p2c_ewma',
      health_check_enabled: false,
      circuit_breaker_enabled: false,
      upstreams: [
        {
          addr: '10.0.1.1:8080',
          enabled: true,
          weight: 1,
          share: 1,
          available: true,
          circuit_open: false,
          circuit_reopens_in_ms: null,
          consecutive_failures: 0,
          probe_healthy: true,
          last_probe_ms_ago: 500,
          inflight: 0,
          ewma_us: 15000
        }
      ]
    }
  ]
});

const received = [];
const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1440, height: 1100 } });

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
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(status()) })
);
await context.route('**/web/upstreams/test', async (route) => {
  const payload = route.request().postDataJSON();
  received.push({ method: 'PROBE', payload });
  await route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({
      addr: payload.addr,
      timeout_ms: payload.timeout_ms ?? 1200,
      healthy: payload.addr === '10.0.0.1:8080',
      latency_ms: payload.addr === '10.0.0.1:8080' ? 7 : null,
      error: payload.addr === '10.0.0.1:8080' ? null : 'connection refused',
      source: 'tcp_connect',
      last_probe_ms_ago: null
    })
  });
});
await context.route('**/admin/services**', async (route) => {
  const request = route.request();
  const method = request.method();
  const name = decodeURIComponent(new URL(request.url()).pathname.replace('/admin/services/', ''));

  if (method === 'GET') {
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(services) });
    return;
  }
  if (method === 'PUT') {
    const payload = request.postDataJSON();
    received.push({ method, name, payload });
    services = services.map((entry) => (entry.name === name ? { ...entry, ...payload } : entry));
    await route.fulfill({ status: 200, contentType: 'text/plain', body: 'service_updated\n' });
    return;
  }
  if (method === 'POST') {
    const payload = request.postDataJSON();
    received.push({ method, payload });
    services = [...services, payload];
    await route.fulfill({ status: 201, contentType: 'text/plain', body: 'service_created\n' });
    return;
  }
  if (method === 'DELETE') {
    received.push({ method, name });
    services = services.filter((entry) => entry.name !== name);
    await route.fulfill({ status: 200, contentType: 'text/plain', body: 'service_deleted\n' });
    return;
  }
  await route.fallback();
});

const page = await context.newPage();
page.setDefaultTimeout(10_000);

const pageErrors = [];
page.on('pageerror', (err) => pageErrors.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  if (msg.type() === 'error') pageErrors.push(`console: ${msg.text()}`);
});

const settle = () => page.waitForTimeout(400);
const card = (name) => page.locator(`[data-service="${name}"]`);

await page.goto(`${ORIGIN}/services`, { waitUntil: 'networkidle' });
await settle();

// --- 1. what the cards say --------------------------------------------------
check((await card('checkout').count()) === 1, 'each service gets a card');
const checkoutText = await card('checkout').innerText();
check(checkoutText.includes('2/2 ready'), 'the card counts ready upstreams from live state');
check(checkoutText.includes('75%'), "the card shows each upstream's share of the ring");
check(
  checkoutText.includes('checkout-web'),
  'the card names the routes pointing at the service'
);
check(
  (await card('search').innerText()).includes('no route points here'),
  'a service nothing routes to says so'
);

// --- 2. live state moves without a reload -----------------------------------
checkoutSecondUpstream = {
  ...checkoutSecondUpstream,
  available: false,
  circuit_open: true,
  circuit_reopens_in_ms: 4200,
  consecutive_failures: 5,
  probe_healthy: false
};
await page.waitForTimeout(3500);
const afterBreak = await card('checkout').innerText();
check(afterBreak.includes('Circuit open'), 'an opening circuit appears without a reload');
check(afterBreak.includes('1/2 ready'), 'and the ready count follows it');

checkoutSecondUpstream = {
  ...checkoutSecondUpstream,
  available: true,
  circuit_open: false,
  circuit_reopens_in_ms: null,
  consecutive_failures: 0,
  probe_healthy: true
};
await page.waitForTimeout(3500);
check(
  (await card('checkout').innerText()).includes('2/2 ready'),
  'recovery shows up the same way'
);

// --- 3. editing keeps what the form did not show ----------------------------
await card('checkout').getByRole('button', { name: 'Edit' }).click();
await settle();
const sheet = page.locator('[data-slot="sheet-content"]');
check(await sheet.isVisible(), 'editing opens the service sheet');
check(new URL(page.url()).pathname === '/services/checkout', 'and deep links to the service');

// The weight slider's share is the page's own arithmetic; it has to match the
// ring the server reports.
check(
  (await sheet.innerText()).includes('75.0% of requests'),
  'the weight slider names the share that weight buys'
);

await sheet.getByRole('button', { name: 'Test upstream 1' }).click();
await settle();
check(
  received.some((entry) => entry.method === 'PROBE' && entry.payload.addr === '10.0.0.1:8080'),
  'the per-upstream test asks the server to probe that one address'
);
check((await sheet.innerText()).includes('Connected in'), 'and shows what came back');

await sheet.getByLabel('Drain upstream 2').click();
await settle();
await sheet.getByRole('button', { name: 'Save changes' }).click();
await settle();

const update = received.findLast((entry) => entry.method === 'PUT');
check(update?.payload?.upstreams?.[1]?.enabled === false, 'draining an upstream reaches the API');
check(
  update?.payload?.health_check?.enabled === true &&
    update?.payload?.health_check?.expected_status?.join(',') === '200,204',
  'the health check survives an edit that never opened the Health tab'
);
check(
  update?.payload?.sticky?.enabled === true && update?.payload?.sticky?.ttl_s === 1800,
  'session affinity survives the same edit'
);
check(
  update?.payload?.retry_budget_ratio === 0.2 && update?.payload?.upstream_h2 === 'never',
  'so do the retry budget and the upstream protocol'
);

// --- 4. deleting a service that routes still use ----------------------------
await page.goto(`${ORIGIN}/services/checkout`, { waitUntil: 'networkidle' });
await settle();
const deleteButton = sheet.getByRole('button', { name: 'Delete service' });
check(await deleteButton.isDisabled(), 'delete is blocked while routes still point at it');
check(
  (await sheet.innerText()).includes('checkout-web'),
  'and the blocking route is named'
);
await page.keyboard.press('Escape');
await settle();

const beforeDelete = received.length;
await card('search').getByRole('button', { name: /^Actions for/ }).click();
await settle();
await page.getByRole('menuitem', { name: 'Delete' }).click();
await settle();
await page.getByRole('button', { name: 'Delete', exact: true }).last().click();
await settle();
check(
  received.slice(beforeDelete).some((entry) => entry.method === 'DELETE' && entry.name === 'search'),
  'a service nothing points at can be deleted'
);

await browser.close();
stop();

if (pageErrors.length > 0) {
  console.error('\nThe page logged errors:\n');
  for (const error of pageErrors) console.error(`  ${error}`);
  process.exit(1);
}

if (failures.length > 0) {
  console.error(`\nServices page check failed — ${failures.length} problem(s):\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('Services page check passed — live state, weights, drain, edits, delete guard.');
process.exit(0);
