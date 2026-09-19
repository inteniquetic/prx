// Loads the built UI out of a running prx and drives it like a person would.
//
// `npm run build` and `npm run check` both pass on code that throws the moment
// it mounts — the Svelte 4 -> 5 migration was exactly that kind of change — so
// this walks the real pages in a real browser and fails on any console error.
// It runs against the binary, which is the only place the UI is actually
// served the way a user gets it: embedded assets, SPA fallback and all.
//
// Usage: npm run smoke -- [base-url]     (default http://127.0.0.1:9090)
import { chromium } from 'playwright';

const base = (process.argv[2] ?? process.env.PRX_ADMIN_URL ?? 'http://127.0.0.1:9090').replace(
  /\/$/,
  ''
);

// The container ships Chromium at a fixed path and blocks `playwright install`,
// so point at it directly rather than letting Playwright resolve a download.
const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const browser = await chromium.launch({ executablePath });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });

const problems = [];
const external = [];
page.on('pageerror', (err) => problems.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  if (msg.type() === 'error') problems.push(`console: ${msg.text()}`);
});
page.on('request', (req) => {
  const { hostname } = new URL(req.url());
  // The UI is embedded in the binary and has to work on a closed network.
  if (!['127.0.0.1', 'localhost'].includes(hostname)) external.push(req.url());
});

// By href, not by name: a menu entry grows a status badge ("Routes 2") the
// moment an upstream goes down, and the test should not care.
const navLink = (path) => page.locator(`nav a[href="${path}"]`).first();
const path = () => new URL(page.url()).pathname;

try {
  await page.goto(base, { waitUntil: 'networkidle' });

  // The shell navigates with real URLs now, so every page is checked for both
  // what it renders and what it leaves in the address bar.
  for (const [label, expected] of [
    ['Services', '/services'],
    ['Routes', '/routes'],
    ['Settings', '/settings'],
    ['Dashboard', '/']
  ]) {
    await navLink(expected).click();
    await page.waitForTimeout(400);
    const heading = (await page.locator('h1, h2').first().innerText()).trim();
    if (heading !== label) problems.push(`${label} page showed heading "${heading}"`);
    if (path() !== expected) problems.push(`${label} left the URL at "${path()}"`);
  }

  // A deep link has to survive the round trip through the server: this is the
  // SPA fallback in src/admin.rs doing its job, which only the binary exercises.
  await navLink('/routes').click();
  await page.waitForTimeout(400);
  await page.locator('tbody tr').first().click();
  await page.waitForTimeout(400);
  const deepLink = page.url();
  if (!path().startsWith('/routes/')) {
    problems.push(`opening a route left the URL at "${path()}"`);
  } else {
    await page.goto(deepLink, { waitUntil: 'networkidle' });
    await page.waitForTimeout(400);
    if (page.url() !== deepLink) problems.push(`reloading ${deepLink} redirected to ${page.url()}`);
    if (!(await page.locator('[data-slot="sheet-content"]').isVisible())) {
      problems.push(`reloading ${deepLink} did not reopen the route`);
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);
  }

  // Opening and dismissing a modal covers the event plumbing (dispatch,
  // window keydown, the backdrop button) that a render-only check misses.
  await navLink('/services').click();
  await page.waitForTimeout(300);
  await page.locator('button').filter({ hasText: /Add Service/i }).first().click();
  await page.waitForTimeout(400);
  if ((await page.locator('[role="dialog"]').count()) !== 1) problems.push('modal did not open');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  if ((await page.locator('[role="dialog"]').count()) !== 0)
    problems.push('Escape did not close the modal');

  // The command palette is the other keyboard surface, and it is portalled and
  // filtered at runtime — plenty of room for a mount-time throw to hide.
  await page.keyboard.press('Control+k');
  await page.waitForTimeout(400);
  if ((await page.locator('[data-slot="command-item"]').count()) === 0) {
    problems.push('the command palette opened empty');
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);

  // Theme: the menu sets it and the choice has to survive a reload without the
  // page flashing the wrong one first.
  for (const [choice, wantsDark] of [
    ['Light', false],
    ['Dark', true]
  ]) {
    await page.getByRole('button', { name: /^Theme:/ }).click();
    await page.waitForTimeout(250);
    await page.getByRole('menuitemradio', { name: choice }).click();
    await page.waitForTimeout(250);
    const isDark = await page.evaluate(() =>
      document.documentElement.classList.contains('dark')
    );
    if (isDark !== wantsDark) problems.push(`${choice} did not apply the right class`);
  }

  const stored = await page.evaluate(() => localStorage.getItem('prx-theme'));
  await page.reload({ waitUntil: 'networkidle' });
  const afterReload = await page.evaluate(() => ({
    choice: localStorage.getItem('prx-theme'),
    dark: document.documentElement.classList.contains('dark')
  }));
  if (afterReload.choice !== stored) problems.push('theme choice did not survive a reload');
  if (stored === 'dark' && !afterReload.dark) problems.push('stored dark theme was not applied on load');
  if (stored === 'light' && afterReload.dark) problems.push('stored light theme was not applied on load');

  // The TOML this UI would write has to be a config the proxy accepts, and it
  // has to still contain the parts of a route the forms never show. This is the
  // one place that can be checked end to end: the page renders the TOML, the
  // real server parses it.
  await navLink('/settings').click();
  await page.waitForTimeout(400);
  // The preview lives behind the TOML tab of the settings page.
  await page.getByRole('button', { name: /TOML Config/ }).click();
  await page.waitForTimeout(400);
  const toml = (await page.locator('pre').first().innerText()).trim();
  if (!toml.includes('[[route]]')) {
    problems.push('the settings page did not render a TOML preview');
  } else {
    const roundTrip = await page.evaluate(async (body) => {
      const before = await (await fetch('/web/config?format=json')).json();
      const put = await fetch('/web/config', {
        method: 'PUT',
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
        body
      });
      const status = put.status;
      const detail = (await put.text()).trim();
      const after = await (await fetch('/web/config?format=json')).json();
      return { status, detail, before, after };
    }, toml);

    if (roundTrip.status !== 200) {
      problems.push(`the proxy rejected the UI's own TOML (${roundTrip.status}): ${roundTrip.detail}`);
    }
    if (roundTrip.before.routes.length !== roundTrip.after.routes.length) {
      problems.push('saving the UI\'s TOML changed how many routes exist');
    }
    for (const route of roundTrip.before.routes) {
      const same = roundTrip.after.routes.find((entry) => entry.name === route.name);
      if (!same) {
        problems.push(`route ${route.name} disappeared after saving the UI's TOML`);
        continue;
      }
      if (route.rate_limit?.enabled !== same.rate_limit?.enabled) {
        problems.push(`route ${route.name} lost its rate limit after saving the UI's TOML`);
      }
      const hadHeaders = JSON.stringify(route.request_headers ?? {});
      if (hadHeaders !== JSON.stringify(same.request_headers ?? {})) {
        problems.push(`route ${route.name} lost its header rules after saving the UI's TOML`);
      }
    }
  }

  // Fonts must come from the binary, never a CDN.
  const fontRequests = await page.evaluate(() =>
    performance.getEntriesByType('resource').filter((r) => r.name.includes('.woff')).map((r) => r.name)
  );
  for (const url of fontRequests) {
    const { hostname } = new URL(url);
    if (!['127.0.0.1', 'localhost'].includes(hostname)) problems.push(`font loaded off-host: ${url}`);
  }

  if (external.length) problems.push(`requested off-host assets: ${external.join(', ')}`);
} catch (err) {
  problems.push(`${err.name}: ${err.message.split('\n')[0]}`);
} finally {
  await browser.close();
}

if (problems.length) {
  console.error(`smoke failed against ${base}:`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}
console.log(
  `smoke passed against ${base}: 4 pages with real URLs, a deep link through the server, ` +
    'modal open/close, command palette, theme menu + persistence, a TOML round trip ' +
    'through the server, self-hosted fonts, no console errors, no external requests'
);
