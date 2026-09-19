// Drives the Routes page against a mocked admin API.
//
// T304 is mostly promises about behaviour under load and about not losing
// data: that search stays instant with 500 routes, that a rejected save lands
// on the field it is about, that editing a route keeps the parts of it the
// form is not showing. None of that is visible to a type checker, and all of
// it is cheap to check in a real browser.
//
// Usage: npm run routes:check
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


const PORT = Number(process.env.ROUTES_PORT ?? 5204);
const ORIGIN = `http://127.0.0.1:${PORT}`;
const ROUTE_COUNT = 500;
const SEARCH_BUDGET_MS = 50;

const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const build = spawn('npx', ['vite', 'build'], {
  cwd: new URL('..', import.meta.url).pathname,
  stdio: ['ignore', 'ignore', 'inherit']
});
const buildCode = await new Promise((resolve) => build.on('exit', resolve));
if (buildCode !== 0) {
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

const services = Array.from({ length: 20 }, (_, i) => ({
  name: `svc-${i + 1}`,
  lb: 'round_robin',
  max_retries: 1,
  retry_backoff_ms: 50,
  circuit_breaker: { enabled: false, consecutive_failures: 5, open_ms: 5000 },
  upstreams: [{ addr: `10.0.0.${i + 1}:8080`, tls: false, sni: '', weight: 1 }]
}));

const baseRoutes = Array.from({ length: ROUTE_COUNT }, (_, i) => ({
  name: `route-${i + 1}`,
  service: `svc-${(i % 20) + 1}`,
  host: `app-${i + 1}.example.com`,
  path_prefix: `/api/v${(i % 3) + 1}`,
  methods: [],
  is_default: i === 0,
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
    cache_status_codes: [200, 203, 300, 301, 404],
    key_query: true,
    vary_headers: ['accept-encoding'],
    coalesce_wait_ms: 2000,
    add_status_header: true
  }
}));

// One route nobody can reach: same host and path as the one above it, no method
// restriction on either. The table has to say so.
baseRoutes.push({
  ...structuredClone(baseRoutes[0]),
  name: 'shadowed',
  is_default: false,
  host: baseRoutes[0].host,
  path_prefix: baseRoutes[0].path_prefix
});

// One route carrying settings the form shows in its Advanced tabs — editing the
// name must not drop them.
baseRoutes.push({
  ...structuredClone(baseRoutes[1]),
  name: 'with-extras',
  host: 'extras.example.com',
  is_default: false,
  request_headers: { set: { 'X-From-Prx': 'yes' }, add: {}, remove: ['X-Internal'] },
  rate_limit: { ...structuredClone(baseRoutes[1].rate_limit), enabled: true, requests_per_second: 25 }
});

// And one that is parked.
baseRoutes.push({
  ...structuredClone(baseRoutes[2]),
  name: 'parked',
  host: 'parked.example.com',
  is_default: false,
  enabled: false
});

let routes = structuredClone(baseRoutes);
const received = [];

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

const health = () => ({
  checked_at_epoch_ms: Date.now(),
  timeout_ms: 1200,
  routes: routes.map((route, index) => ({
    route_index: index,
    name: route.name,
    service: route.service,
    host: route.host,
    path_prefix: route.path_prefix,
    healthy: index % 50 !== 0,
    reachable_upstreams: index % 50 === 0 ? 0 : 1,
    total_upstreams: 1,
    upstreams: []
  }))
});

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });

/** Set to a message to make the next write fail, the way the proxy would. */
let rejectNextWith = null;

await mockDraftEndpoints(context, { toml: DRAFT_TOML, config });
await context.route('**/web/config*', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(config()) })
);
await context.route('**/web/health/routes*', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(health()) })
);
await context.route('**/admin/routes**', async (route) => {
  const request = route.request();
  const method = request.method();
  const url = new URL(request.url());
  const name = decodeURIComponent(url.pathname.replace('/admin/routes/', ''));

  if (method === 'GET') {
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(routes) });
    return;
  }

  if (rejectNextWith) {
    const body = rejectNextWith;
    rejectNextWith = null;
    await route.fulfill({ status: 400, contentType: 'text/plain', body });
    return;
  }

  if (method === 'POST') {
    const payload = request.postDataJSON();
    received.push({ method, payload });
    routes = [...routes, payload];
    await route.fulfill({ status: 201, contentType: 'text/plain', body: 'route_created\n' });
    return;
  }

  if (method === 'PUT') {
    const payload = request.postDataJSON();
    received.push({ method, name, payload });
    routes = routes.map((entry) => (entry.name === name ? { ...entry, ...payload } : entry));
    await route.fulfill({ status: 200, contentType: 'text/plain', body: 'route_updated\n' });
    return;
  }

  if (method === 'DELETE') {
    received.push({ method, name });
    routes = routes.filter((entry) => entry.name !== name);
    await route.fulfill({ status: 200, contentType: 'text/plain', body: 'route_deleted\n' });
    return;
  }

  await route.fallback();
});

const page = await context.newPage();
page.setDefaultTimeout(10_000);

const pageErrors = [];
page.on('pageerror', (err) => pageErrors.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  // The rejection cases below make the admin API answer 400 on purpose, and the
  // browser logs every failed request. Those are the test working, not the app
  // breaking.
  const text = msg.text();
  if (msg.type() === 'error' && !text.includes('status of 400')) {
    pageErrors.push(`console: ${text}`);
  }
});

const settle = () => page.waitForTimeout(350);
const rows = () => page.locator('tbody tr');
const search = () => page.getByLabel('Search routes');

await page.goto(`${ORIGIN}/routes`, { waitUntil: 'networkidle' });
await settle();

// --- 1. the table ----------------------------------------------------------
check((await rows().count()) === 25, 'the table paginates to 25 rows');
check(
  (await page.getByTestId('result-count').innerText()).includes(String(routes.length)),
  'the count reflects the whole config'
);

// --- 2. search latency with 500 routes -------------------------------------
// Measured in the page from the keystroke to the row list settling, so it is
// the app's own work being timed rather than the driver's round trips.
const searchMs = await page.evaluate(async () => {
  const input = document.querySelector('input[aria-label="Search routes"]');
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    'value'
  ).set;

  const start = performance.now();
  setter.call(input, 'route-4');
  input.dispatchEvent(new Event('input', { bubbles: true }));

  // Two frames: one for Svelte to react, one for the browser to paint it.
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return performance.now() - start;
});
check(
  searchMs < SEARCH_BUDGET_MS,
  `search responds in ${searchMs.toFixed(1)}ms with ${routes.length} routes (budget ${SEARCH_BUDGET_MS}ms)`
);

await search().fill('extras');
await settle();
check((await rows().count()) === 1, 'search narrows to the matching route');

await search().fill('app-7.example.com');
await settle();
check((await rows().count()) >= 1, 'search matches on host as well as name');

await search().fill('');
await settle();

// --- 3. shadowing warning --------------------------------------------------
await search().fill('shadowed');
await settle();
const warningIcon = rows().first().locator('[aria-label="This route has a warning"]');
check(await warningIcon.isVisible(), 'an unreachable route is flagged in the table');
await warningIcon.hover();
await page.waitForTimeout(500);
check(
  (await page.locator('[data-slot="tooltip-content"]').innerText()).toLowerCase().includes('never matches'),
  'the warning says why the route never matches'
);

// --- 4. editing keeps what the form is not showing -------------------------
await search().fill('with-extras');
await settle();
await rows().first().click();
await settle();
check(new URL(page.url()).pathname === '/routes/with-extras', 'opening a route deep links to it');

// Scoped to the sheet: the column header carries a "Sort by Name" button, and
// a required field's accessible name ends with "(required)".
const sheet = page.locator('[data-slot="sheet-content"]');
const nameField = sheet.getByLabel('Name').first();
await nameField.fill('with-extras-renamed');
await page.getByRole('button', { name: 'Save changes' }).click();
await settle();

const update = received.findLast((entry) => entry.method === 'PUT');
check(update?.payload?.name === 'with-extras-renamed', 'the rename reaches the admin API');
check(
  update?.payload?.rate_limit?.enabled === true &&
    update?.payload?.rate_limit?.requests_per_second === 25,
  'the rate limit survives an edit that never opened the Limits tab'
);
check(
  update?.payload?.request_headers?.set?.['X-From-Prx'] === 'yes' &&
    update?.payload?.request_headers?.remove?.[0] === 'X-Internal',
  'header rules survive the same edit'
);

// --- 5. a rejected save lands on the field ---------------------------------
await page.goto(`${ORIGIN}/routes`, { waitUntil: 'networkidle' });
await settle();
await search().fill('route-12');
await settle();
await rows().first().click();
await settle();

// A rejection about a field that is on screen is shown at that field...
rejectNextWith = "route 'route-12' path_prefix must start with '/'\n";
await page.getByRole('button', { name: 'Save changes' }).click();
await settle();
check(
  (await sheet.locator('[data-slot="form-message"]').first().innerText()).includes('path_prefix'),
  "the server's complaint is shown at the field it names"
);

// ...and one about a field behind a collapsed section is still readable.
rejectNextWith = "route 'route-12' cache.ttl_ms must be > 0\n";
await page.getByRole('button', { name: 'Save changes' }).click();
await settle();
check(
  (await sheet.locator('[data-slot="alert"]').first().innerText()).includes('cache.ttl_ms'),
  'a rejection about a hidden field is still shown'
);

// Client-side validation blocks the save before the server ever sees it.
await page.getByRole('tab', { name: /Matching/ }).click();
await settle();
await sheet.getByLabel('Path prefix').first().fill('api');
await settle();
check(
  await page.getByRole('button', { name: 'Save changes' }).isDisabled(),
  'a path prefix without a leading slash blocks the save'
);
check(
  (await page.locator('[data-slot="form-message"]').first().innerText()).includes('/'),
  'and says what is wrong with it'
);
await page.getByRole('button', { name: 'Cancel' }).click();
await settle();

// --- 6. bulk actions -------------------------------------------------------
await search().fill('route-1');
await settle();
const before = received.length;
await rows().nth(0).getByRole('checkbox').click();
await rows().nth(1).getByRole('checkbox').click();
await settle();
check(
  (await page.getByText('2 selected').count()) === 1,
  'selecting rows shows the bulk action bar'
);
await page.getByRole('button', { name: 'Disable' }).click();
await settle();
const disables = received.slice(before).filter((entry) => entry.method === 'PUT');
check(disables.length === 2, 'disabling two routes sends two updates');
check(
  disables.every((entry) => entry.payload.enabled === false),
  'and asks for exactly that'
);

// --- 6b. the row action menu -----------------------------------------------
// It mounts on demand to keep the table cheap, so opening one and using it is
// worth asserting rather than assuming.
await page.goto(`${ORIGIN}/routes`, { waitUntil: 'networkidle' });
await settle();
await search().fill('with-extras-renamed');
await settle();
const beforeDuplicate = received.length;
await page.getByRole('button', { name: /^Actions for/ }).first().click();
await settle();
check((await page.getByRole('menu').count()) > 0, 'the row action menu opens');
await page.getByRole('menuitem', { name: 'Duplicate' }).click();
await settle();
const duplicate = received.slice(beforeDuplicate).find((entry) => entry.method === 'POST');
check(
  duplicate?.payload?.name === 'with-extras-renamed-copy',
  'duplicating a route posts a copy under a free name'
);
check(duplicate?.payload?.is_default === false, 'and the copy is not a second default route');

// --- 7. the route tester ---------------------------------------------------
await context.route('**/web/routes/test', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify({
      outcome: 'matched',
      request: {
        method: 'GET',
        host: 'app-1.example.com',
        normalized_host: 'app-1.example.com',
        path: '/api/v1'
      },
      route: {
        index: 0,
        name: 'route-1',
        host: 'app-1.example.com',
        path_prefix: '/api/v1',
        methods: [],
        is_default: true,
        enabled: true,
        matched_by: 'exact_host'
      },
      service: {
        name: 'svc-1',
        lb: 'round_robin',
        selection: {
          deterministic: true,
          would_pick: '10.0.0.1:8080',
          note: 'Round robin: the next request takes the following upstream in the ring.'
        },
        upstreams: [
          {
            addr: '10.0.0.1:8080',
            weight: 1,
            available: true,
            circuit_open: false,
            probe_healthy: true,
            inflight: 0,
            ewma_us: 8200
          }
        ]
      }
    })
  })
);

await page.goto(`${ORIGIN}/routes`, { waitUntil: 'networkidle' });
await settle();
await page.getByRole('button', { name: 'Test a request' }).click();
await settle();
await page.getByLabel('Host', { exact: true }).fill('app-1.example.com');
await page.getByLabel('Path', { exact: true }).fill('/api/v1');
await page.getByRole('button', { name: 'Test', exact: true }).click();
await settle();
const testerText = await page.locator('[data-slot="alert"]').first().innerText();
check(testerText.includes('route-1'), 'the tester names the route a request would reach');
check(testerText.toLowerCase().includes('exact host'), 'and which rule won it');
check(
  (await page.getByText('10.0.0.1:8080').count()) > 0,
  'the tester shows the upstream that would take it'
);

await browser.close();
stop();

if (pageErrors.length > 0) {
  console.error('\nThe page logged errors:\n');
  for (const error of pageErrors) console.error(`  ${error}`);
  process.exit(1);
}

if (failures.length > 0) {
  console.error(`\nRoutes page check failed — ${failures.length} problem(s):\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('Routes page check passed — table, search budget, warnings, form, bulk, tester.');
process.exit(0);
