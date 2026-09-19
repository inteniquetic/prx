// Loads the built UI out of a running prx and drives it like a person would.
//
// `npm run build` and `npm run check` both pass on code that throws the moment
// it mounts — the Svelte 4 -> 5 migration is exactly that kind of change — so
// this walks the real pages in a real browser and fails on any console error.
//
// Usage: npm run smoke -- [base-url]     (default http://127.0.0.1:9090)
import { chromium } from 'playwright';

const base = process.argv[2] ?? process.env.PRX_ADMIN_URL ?? 'http://127.0.0.1:9090';

// The container ships Chromium at a fixed path and blocks `playwright install`,
// so point at it directly rather than letting Playwright resolve a download.
const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const browser = await chromium.launch({ executablePath });
const page = await browser.newPage();

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

const navButton = (label) =>
  page.locator('nav button, aside button').filter({ hasText: label }).first();

try {
  await page.goto(base, { waitUntil: 'networkidle' });

  for (const label of ['Services', 'Routes', 'Settings', 'Dashboard']) {
    await navButton(label).click();
    await page.waitForTimeout(400);
    const heading = (await page.locator('h1, h2').first().innerText()).trim();
    if (heading !== label) problems.push(`${label} page showed heading "${heading}"`);
  }

  // Opening and dismissing a modal covers the event plumbing (dispatch,
  // window keydown, the backdrop button) that a render-only check misses.
  await navButton('Services').click();
  await page.waitForTimeout(300);
  await page.locator('button').filter({ hasText: /Add Service/i }).first().click();
  await page.waitForTimeout(400);
  if ((await page.locator('[role="dialog"]').count()) !== 1) problems.push('modal did not open');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  if ((await page.locator('[role="dialog"]').count()) !== 0) problems.push('Escape did not close the modal');

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
console.log(`smoke passed against ${base}: 4 pages, modal open/close, no console errors, no external requests`);
