// Drives the app shell in a real browser against a mocked admin API.
//
// Every claim T303 makes is about behaviour a type checker cannot see: that a
// deep link survives a refresh, that a keyboard alone gets you from a cold load
// to editing a route, that nothing spills sideways at 375px, and that the
// command palette still opens instantly with 500 routes in the config. So this
// asserts all four, with a config big enough for the last one to mean anything.
//
// Usage: npm run shell:check
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';
import { MEASURE } from './measure-contrast.mjs';

const PORT = Number(process.env.SHELL_PORT ?? 5202);
const ORIGIN = `http://127.0.0.1:${PORT}`;
const ROUTE_COUNT = 500;
const SERVICE_COUNT = 50;
const PALETTE_BUDGET_MS = 100;

const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const server = spawn(
  'npx',
  ['vite', '--port', String(PORT), '--strictPort', '--host', '127.0.0.1'],
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

const services = Array.from({ length: SERVICE_COUNT }, (_, i) => ({
  name: `svc-${i + 1}`,
  lb: 'round_robin',
  max_retries: 1,
  retry_backoff_ms: 50,
  circuit_breaker: { enabled: false, consecutive_failures: 5, open_ms: 5000 },
  upstreams: [
    { addr: `10.0.${Math.floor(i / 256)}.${i % 256}:8080`, tls: false, sni: '', weight: 1 }
  ]
}));

const routes = Array.from({ length: ROUTE_COUNT }, (_, i) => ({
  name: `route-${i + 1}`,
  service: `svc-${(i % SERVICE_COUNT) + 1}`,
  host: `app-${i + 1}.example.com`,
  path_prefix: `/api/v${(i % 3) + 1}`,
  methods: [],
  is_default: i === 0
}));

const config = {
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
};

const health = {
  checked_at_epoch_ms: Date.now(),
  timeout_ms: 1200,
  routes: routes.map((route, index) => ({
    route_index: index,
    name: route.name,
    service: route.service,
    host: route.host,
    path_prefix: route.path_prefix,
    // A couple of sick routes so the sidebar badges have something to show.
    healthy: index % 97 !== 0,
    reachable_upstreams: index % 97 === 0 ? 0 : 1,
    total_upstreams: 1,
    upstreams: []
  }))
};

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });

await context.route('**/web/config*', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(config) })
);
await context.route('**/web/health/routes*', (route) =>
  route.fulfill({ contentType: 'application/json', body: JSON.stringify(health) })
);

const page = await context.newPage();
page.setDefaultTimeout(10_000);

const pageErrors = [];
page.on('pageerror', (err) => pageErrors.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  if (msg.type() === 'error') pageErrors.push(`console: ${msg.text()}`);
});
page.on('response', (response) => {
  if (response.status() >= 400) pageErrors.push(`http ${response.status()}: ${response.url()}`);
});

const settle = () => page.waitForTimeout(350);

// Vite discovers and pre-bundles dependencies on the first load and answers 504
// to anything in flight while it does. That is a dev-server artefact, not the
// app, so the first load is a warm-up and only what happens after it counts.
await page.goto(ORIGIN, { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);
await page.reload({ waitUntil: 'networkidle' });
await page.waitForTimeout(500);
pageErrors.length = 0;

// --- 1. deep links ---------------------------------------------------------
await page.goto(`${ORIGIN}/routes/route-42`, { waitUntil: 'networkidle' });
await settle();
check(new URL(page.url()).pathname === '/routes/route-42', 'deep link keeps its URL');
check(
  (await page.getByText('route-42', { exact: false }).count()) > 0,
  'deep link opens the named route'
);

await page.reload({ waitUntil: 'networkidle' });
await settle();
check(new URL(page.url()).pathname === '/routes/route-42', 'the route survives a refresh');
check(
  (await page.getByRole('button', { name: 'Back to Routes' }).count()) > 0 &&
    (await page.locator('input').first().inputValue()) === 'route-42',
  'the refreshed page still shows the route detail'
);

// Back/forward have to work like they do anywhere else on the web.
await page.goBack({ waitUntil: 'load' });
await settle();
await page.goForward({ waitUntil: 'load' });
await settle();
check(new URL(page.url()).pathname === '/routes/route-42', 'forward returns to the route');

// A name that is not in the config drops back to the list instead of 404ing.
await page.goto(`${ORIGIN}/routes/does-not-exist`, { waitUntil: 'networkidle' });
await settle();
check(new URL(page.url()).pathname === '/routes', 'an unknown route name falls back to the list');

// --- 2. command palette ----------------------------------------------------
await page.goto(ORIGIN, { waitUntil: 'networkidle' });
await settle();

const openMs = await page.evaluate(async () => {
  const appeared = new Promise((resolve) => {
    const observer = new MutationObserver(() => {
      if (document.querySelector('[data-slot="command-item"]')) {
        observer.disconnect();
        resolve(performance.now());
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
  const start = performance.now();
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }));
  return (await appeared) - start;
});
check(
  openMs < PALETTE_BUDGET_MS,
  `palette opens in ${openMs.toFixed(1)}ms with ${ROUTE_COUNT} routes (budget ${PALETTE_BUDGET_MS}ms)`
);

// Searching by host, not just by name — that is how anyone actually looks for
// a route they only know by the domain it serves.
await page.keyboard.type('app-321.example.com');
await page.waitForTimeout(200);
const hostHit = page.locator('[data-slot="command-item"]', { hasText: 'route-321' });
check((await hostHit.count()) > 0, 'the palette finds a route by its host');
await hostHit.first().click();
await settle();
check(new URL(page.url()).pathname === '/routes/route-321', 'the palette navigates to the route');

// --- 2b. the topbar menus --------------------------------------------------
// Menus are built from context that only exists once they open, so a wrong
// nesting throws on the first click and nowhere earlier.
await page.getByRole('button', { name: /^Theme:/ }).click();
await page.waitForTimeout(300);
check((await page.getByRole('menuitemradio').count()) === 3, 'the theme menu offers three choices');
await page.getByRole('menuitemradio', { name: 'Dark' }).click();
await page.waitForTimeout(300);
check(
  await page.evaluate(() => document.documentElement.classList.contains('dark')),
  'picking Dark applies the dark theme'
);
await page.getByRole('button', { name: 'Account' }).click();
await page.waitForTimeout(300);
check((await page.getByRole('menuitem').count()) > 0, 'the account menu opens');
await page.keyboard.press('Escape');
await page.waitForTimeout(250);

// --- 3. keyboard only ------------------------------------------------------
await page.goto(ORIGIN, { waitUntil: 'networkidle' });
await settle();

await page.keyboard.press('Tab');
const firstStop = await page.evaluate(() => document.activeElement?.textContent?.trim());
check(firstStop === 'Skip to content', 'the first tab stop is the skip link');

let reachedRoutes = false;
for (let i = 0; i < 25 && !reachedRoutes; i += 1) {
  await page.keyboard.press('Tab');
  reachedRoutes = await page.evaluate(
    () => document.activeElement?.getAttribute('href') === '/routes'
  );
}
check(reachedRoutes, 'tabbing reaches the Routes link');
await page.keyboard.press('Enter');
await settle();
check(new URL(page.url()).pathname === '/routes', 'Enter on the link navigates');

// Walk to the first route in the table and open it, still without a mouse.
let openedDetail = false;
for (let i = 0; i < 40 && !openedDetail; i += 1) {
  await page.keyboard.press('Tab');
  const isRouteRow = await page.evaluate(() => {
    const el = document.activeElement;
    return Boolean(el && el.closest('tbody') && el.tagName !== 'INPUT');
  });
  if (!isRouteRow) continue;
  await page.keyboard.press('Enter');
  await settle();
  openedDetail = new URL(page.url()).pathname.startsWith('/routes/');
}
check(openedDetail, 'a route opens from the keyboard');

if (openedDetail) {
  // And the detail form takes typing — the end of "open the app, edit a route".
  const nameField = page.locator('input[type="text"]').first();
  await nameField.focus();
  await page.keyboard.press('End');
  await page.keyboard.type('-edited');
  check((await nameField.inputValue()).endsWith('-edited'), 'the route form accepts typing');
}

// --- 4. 375px --------------------------------------------------------------
await page.setViewportSize({ width: 375, height: 720 });
for (const path of ['/', '/routes', '/services', '/settings', '/tls', '/audit']) {
  await page.goto(`${ORIGIN}${path}`, { waitUntil: 'networkidle' });
  await settle();
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    return doc.scrollWidth - doc.clientWidth;
  });
  check(overflow <= 1, `no horizontal scroll at 375px on ${path} (overflow ${overflow}px)`);
}

// The rail is gone on a phone, so the drawer has to carry the whole nav.
await page.goto(ORIGIN, { waitUntil: 'networkidle' });
await settle();
check(
  !(await page.locator('[data-slot="sidebar"]').first().isVisible()),
  'the sidebar rail is hidden at 375px'
);
await page.getByRole('button', { name: 'Open navigation' }).click();
await page.waitForTimeout(400);
const drawer = page.locator('[data-slot="sheet-content"]');
check(await drawer.isVisible(), 'the navigation drawer opens');
await drawer.locator('a[href="/services"]').first().click();
await settle();
check(new URL(page.url()).pathname === '/services', 'the drawer navigates');
check(!(await drawer.isVisible()), 'the drawer closes after navigating');

// --- 5. contrast of the chrome --------------------------------------------
// Scoped to what this task owns: the rail, the topbar, the drawer and the
// palette. The pages inside are rewritten by T304-T308 and are audited with
// their own work, not claimed here.
const colourSource = (await readFile(new URL('./contrast.mjs', import.meta.url), 'utf8')).replace(
  /^export /gm,
  ''
);
const CHROME = ['[data-slot="sidebar"]', '[data-slot="topbar"]'];

await page.setViewportSize({ width: 1280, height: 900 });
for (const theme of ['light', 'dark']) {
  await page.goto(`${ORIGIN}/routes`, { waitUntil: 'networkidle' });
  await page.evaluate((value) => localStorage.setItem('prx-theme', value), theme);
  await page.reload({ waitUntil: 'networkidle' });
  await settle();
  await page.addScriptTag({ content: colourSource });

  const rows = await page.evaluate(MEASURE, CHROME);
  const bad = rows.filter((row) => row.ratio < row.min);
  check(
    bad.length === 0,
    `the shell chrome is AA in ${theme} (${rows.length} text runs${
      bad.length ? `, worst ${bad[0].ratio.toFixed(2)}:1 on ${bad[0].what}` : ''
    })`
  );

  // The palette is the one piece of chrome that only exists while open.
  await page.keyboard.press('Control+k');
  await page.waitForTimeout(400);
  const paletteRows = await page.evaluate(MEASURE, ['[data-slot="dialog-content"]']);
  const paletteBad = paletteRows.filter((row) => row.ratio < row.min);
  check(
    paletteRows.length > 0 && paletteBad.length === 0,
    `the command palette is AA in ${theme} (${paletteRows.length} text runs${
      paletteBad.length ? `, worst ${paletteBad[0].ratio.toFixed(2)}:1 on ${paletteBad[0].what}` : ''
    })`
  );
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
}

await browser.close();
stop();

if (pageErrors.length > 0) {
  console.error('\nThe app logged errors:\n');
  for (const error of pageErrors) console.error(`  ${error}`);
  process.exit(1);
}

if (failures.length > 0) {
  console.error(`\nApp shell check failed — ${failures.length} problem(s):\n`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('App shell check passed — deep links, keyboard, 375px and palette budget.');
process.exit(0);
