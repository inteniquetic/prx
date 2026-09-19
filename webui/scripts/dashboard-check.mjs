// Drives the Dashboard against a mocked live-stats API.
//
// The page claims four things a type checker cannot see: that the numbers move
// on their own from the stream, that a degraded proxy is impossible to miss and
// says where to go, that a stream which goes away is announced rather than
// leaving a chart quietly frozen, and that a tab left open does not grow
// without bound.
//
// Usage: npm run dashboard:check
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


const PORT = Number(process.env.DASHBOARD_PORT ?? 5206);
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
    config_reload_debounce_ms: 300,
    tls: null
  },
  observability: { log_level: 'info', access_log: true, prometheus_listen: '127.0.0.1:9100' },
  services: [
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
      upstreams: [upstream('10.0.0.1:8080'), upstream('10.0.0.2:8080')]
    }
  ],
  routes: [
    {
      name: 'checkout-api',
      service: 'checkout',
      host: 'shop.example.com',
      path_prefix: '/api',
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
        enabled: true,
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
  ]
});

// --- the live stats it serves ----------------------------------------------

let seq = 0;
let rps = 120;
/** Flipped mid-run so the page has a degradation to notice. */
let degraded = false;

const sample = () => {
  seq += 1;
  rps += 5;
  return {
    epoch_ms: Date.now(),
    window_seconds: 1,
    rps,
    rps_2xx: rps * 0.9,
    rps_3xx: rps * 0.02,
    rps_4xx: rps * 0.05,
    rps_5xx: degraded ? rps * 0.08 : rps * 0.03,
    error_ratio_4xx: 0.05,
    error_ratio_5xx: degraded ? 0.08 : 0.03,
    p50_ms: 3.5,
    p95_ms: 21,
    p99_ms: degraded ? 180 : 48,
    inflight: 7,
    upstreams_healthy: degraded ? 1 : 2,
    upstreams_total: 2,
    cache_hit_ratio: 0.62
  };
};

const routes = () => [
  {
    name: 'checkout-api',
    requests: 5400,
    rps,
    error_ratio: degraded ? 0.08 : 0.03,
    requests_4xx: 120,
    requests_5xx: degraded ? 430 : 60,
    p50_ms: 3.5,
    p95_ms: 21,
    p99_ms: degraded ? 180 : 48,
    window_seconds: 60
  },
  {
    name: 'static-assets',
    requests: 900,
    rps: 15,
    error_ratio: 0,
    requests_4xx: 0,
    requests_5xx: 0,
    p50_ms: 1.2,
    p95_ms: 4,
    p99_ms: 9,
    window_seconds: 60
  }
];

const services = () => [
  {
    name: 'checkout',
    healthy: degraded ? 1 : 2,
    total: 2,
    upstreams: [
      { addr: '10.0.0.1:8080', state: 'healthy', inflight: 4, ewma_ms: 8.2 },
      {
        addr: '10.0.0.2:8080',
        state: degraded ? 'down' : 'healthy',
        inflight: 0,
        ewma_ms: 9.1
      }
    ]
  }
];

const snapshot = () => ({
  epoch_ms: Date.now(),
  interval_ms: 1000,
  seq,
  uptime_seconds: 3600,
  sample: sample(),
  history: Array.from({ length: 90 }, () => sample()),
  routes: routes(),
  services: services(),
  events: [
    {
      id: 2,
      epoch_ms: Date.now() - 30_000,
      level: 'info',
      kind: 'config_apply',
      message: 'Config applied from the admin API',
      target: null
    }
  ],
  stream_clients: 1,
  max_stream_clients: 16
});

/** One SSE response body: a retry hint and a burst of ticks. */
const streamBody = (ticks) => {
  let body = 'retry: 60\n\n';
  body += `event: hello\ndata: ${seq}\n\n`;
  for (let index = 0; index < ticks; index += 1) {
    const payload = {
      seq: seq + 1,
      sample: sample(),
      routes: routes(),
      services: services(),
      events:
        index === 0 && degraded
          ? [
              {
                id: 1000 + seq,
                epoch_ms: Date.now(),
                level: 'error',
                kind: 'circuit_open',
                message: 'Circuit opened for checkout/10.0.0.2:8080',
                target: 'checkout'
              }
            ]
          : []
    };
    body += `id: ${payload.seq}\nevent: tick\ndata: ${JSON.stringify(payload)}\n\n`;
  }
  return body;
};

/** Switched mid-run to test the fallbacks. */
let streamMode = 'ok'; // 'ok' | 'refused' | 'down'
let statsMode = 'ok'; // 'ok' | 'down'

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1600, height: 1200 } });

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
await context.route('**/web/stats/stream', (route) => {
  if (streamMode === 'refused') {
    return route.fulfill({
      status: 503,
      contentType: 'text/plain',
      body: 'too_many_stats_clients\n'
    });
  }
  if (streamMode === 'down') {
    return route.abort('connectionrefused');
  }
  return route.fulfill({
    status: 200,
    headers: { 'content-type': 'text/event-stream', 'cache-control': 'no-store' },
    body: streamBody(30)
  });
});
await context.route('**/web/stats', (route) => {
  if (statsMode === 'down') {
    return route.abort('connectionrefused');
  }
  return route.fulfill({ contentType: 'application/json', body: JSON.stringify(snapshot()) });
});

const page = await context.newPage();
page.setDefaultTimeout(10_000);

const pageErrors = [];
page.on('pageerror', (err) => pageErrors.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  if (msg.type() !== 'error') return;
  // The last phases take the admin API away on purpose; the browser logs the
  // refused stream and the failed fetch itself, and those are the thing being
  // tested rather than a fault in the page.
  if (/Failed to load resource/.test(msg.text())) return;
  pageErrors.push(`console: ${msg.text()}`);
});

const settle = (ms = 500) => page.waitForTimeout(ms);
const tile = (label) => page.locator('[data-slot="metric-tile"]', { hasText: label }).first();
const banner = page.locator('[data-system-state]');

await page.goto(`${ORIGIN}/`, { waitUntil: 'networkidle' });
await settle(900);

// --- 1. the tiles read the live sample --------------------------------------
check((await tile('Requests').count()) === 1, 'the top row has a requests tile');
check(
  /req\/s/.test(await tile('Requests').innerText()),
  'the requests tile is in requests per second'
);
check(
  (await tile('p99 latency').innerText()).includes('ms'),
  'the latency tile is in milliseconds'
);
check((await tile('5xx rate').count()) === 1, '5xx has its own tile');
check((await tile('4xx rate').count()) === 1, 'and 4xx is counted separately');
check(
  (await tile('Cache hits').count()) === 1,
  'the cache tile appears because a route has caching on'
);

// Every number says where it came from (T306 acceptance).
const hints = await page.locator('[data-slot="metric-tile"] [aria-label$="where this number comes from"]').count();
check(hints >= 5, `every tile carries a metric hint (found ${hints})`);
await page.locator('[data-slot="metric-tile"] [aria-label$="where this number comes from"]').first().hover();
await settle(400);
check(
  (await page.locator('[data-slot="tooltip-content"]').first().innerText()).includes(
    'prx_requests_total'
  ),
  'the hint names the metric the number is read from'
);
await page.mouse.move(0, 0);

// --- 2. the numbers move on their own ---------------------------------------
const before = await tile('Requests').innerText();
await settle(1500);
const after = await tile('Requests').innerText();
check(before !== after, 'the stream moves the numbers without a reload');

// --- 3. the charts are drawn -------------------------------------------------
const trafficChart = page.locator('figure', { hasText: 'Requests per second' }).first();
check(await trafficChart.isVisible(), 'the traffic chart is on the page');
check(
  (await trafficChart.locator('path[fill="currentColor"]').count()) >= 4,
  'traffic is stacked by status class'
);
const latencyChart = page.locator('figure', { hasText: 'Latency percentiles' }).first();
check(
  (await latencyChart.innerText()).includes('p99'),
  'the latency chart names its percentiles rather than relying on colour'
);

// The crosshair is the chart's own tooltip: hovering must name a value.
await latencyChart.locator('button').hover();
await settle(300);
check(
  (await latencyChart.innerText()).includes('ago') ||
    (await latencyChart.innerText()).includes('now'),
  'hovering the chart reads out the sample under the cursor'
);

// --- 4. the leaderboards --------------------------------------------------
const top = page.locator('[data-leaderboard="traffic"]');
check(
  (await top.locator('[data-route="checkout-api"]').count()) === 1,
  'the busiest route is listed'
);
await top.locator('[data-route="checkout-api"] button').first().click();
await settle();
check(
  new URL(page.url()).pathname === '/routes/checkout-api',
  'clicking a route opens it on the Routes page'
);
await page.goBack();
await settle(900);

// --- 5. a degraded proxy is impossible to miss ------------------------------
check(
  (await banner.getAttribute('data-system-state')) === 'ok',
  'a healthy proxy says so plainly'
);
degraded = true;
await settle(2500);
check(
  (await banner.getAttribute('data-system-state')) === 'degraded',
  'losing an upstream turns the banner'
);
const bannerText = await banner.innerText();
check(bannerText.includes('checkout'), 'and the banner names the service that is hurting');
check(
  (await banner.getByRole('button', { name: /Open service/ }).count()) > 0,
  'and links to where it can be fixed'
);
check(
  (await page.locator('[data-event-kind="circuit_open"]').count()) > 0,
  'the circuit trip lands on the event strip'
);
await banner.getByRole('button', { name: /Open service/ }).first().click();
await settle();
check(
  new URL(page.url()).pathname === '/services/checkout',
  'the banner link goes to the service itself'
);
await page.goto(`${ORIGIN}/`, { waitUntil: 'networkidle' });
await settle(900);

// --- 6. an hour on screen must not grow without bound -----------------------
const measure = async () =>
  page.evaluate(() => ({
    nodes: document.querySelectorAll('*').length,
    path:
      document.querySelector('figure path[stroke="currentColor"]')?.getAttribute('d')?.length ?? 0
  }));
await settle(2000);
const early = await measure();
// Roughly ten minutes of samples at the mocked rate, which is past the point
// where an uncapped window would still be growing.
await settle(6000);
const later = await measure();
check(
  later.path <= early.path * 1.15,
  `the chart's window is capped, not growing (${early.path} → ${later.path} path chars)`
);
check(
  later.nodes <= early.nodes + 40,
  `the DOM does not grow with time (${early.nodes} → ${later.nodes} nodes)`
);

// --- 7. a refused stream falls back rather than freezing --------------------
streamMode = 'refused';
await settle(3000);
check(
  (await page.locator('[data-slot="live-status"]').innerText()).trim() === 'polling',
  'a refused stream falls back to polling and says so'
);
const polled = await tile('Requests').innerText();
await settle(2500);
check(polled !== (await tile('Requests').innerText()), 'and the numbers keep moving');

// --- 8. an admin API that goes away is announced ----------------------------
streamMode = 'down';
statsMode = 'down';
await settle(6000);
const state = await banner.getAttribute('data-system-state');
check(
  state === 'reconnecting' || state === 'offline',
  `a dead admin API is announced, not shown as a frozen chart (state was ${state})`
);
check(
  /Reconnecting|Lost contact/.test(await banner.innerText()),
  'and the banner says so in words'
);
check(
  await trafficChart.isVisible(),
  'the last known chart stays on screen behind the warning'
);

await browser.close();
stop();

if (pageErrors.length > 0) {
  console.error('\nThe page logged errors:\n');
  for (const error of pageErrors) console.error(`  ${error}`);
  process.exit(1);
}

if (failures.length > 0) {
  console.error(`\nDashboard check failed — ${failures.length} problem(s):\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('Dashboard check passed — live tiles, charts, leaderboards, degradation, fallbacks.');
process.exit(0);
