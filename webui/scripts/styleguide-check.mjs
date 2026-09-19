// Drives the styleguide in a real browser and measures the contrast of what is
// actually painted, in light and in dark.
//
// The token check (scripts/contrast-check.mjs) proves the palette is sound; it
// cannot prove a component reached for the wrong pair of tokens, stacked a
// translucent fill over another one, or dimmed text with an opacity utility.
// This walks every visible run of text on the page — overlays opened on the
// way — and computes the ratio from getComputedStyle, which is the number the
// person in front of the screen actually gets.
//
// Usage: npm run styleguide:check
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const PORT = Number(process.env.STYLEGUIDE_PORT ?? 5199);
const URL_ = `http://127.0.0.1:${PORT}/?styleguide`;

// The container ships Chromium at a fixed path and blocks `playwright install`.
const executablePath =
  process.env.PLAYWRIGHT_CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const server = spawn(
  'npx',
  ['vite', '--port', String(PORT), '--strictPort', '--host', '127.0.0.1'],
  {
    cwd: new URL('..', import.meta.url).pathname,
    stdio: ['ignore', 'ignore', 'pipe'],
    // Its own group: npx keeps vite in a child of its own, and killing only npx
    // leaves vite holding the port and this script waiting on it forever.
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
      const res = await fetch(URL_);
      if (res.ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error('vite did not come up in time');
}

// The same colour maths the token check uses, handed to the page verbatim so
// the two checks cannot drift apart.
const colourSource = (await readFile(new URL('./contrast.mjs', import.meta.url), 'utf8')).replace(
  /^export /gm,
  ''
);

const MEASURE = () => {
  const results = [];

  const hidden = (el) => {
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none') return true;
    if (parseFloat(style.opacity) < 0.95) return true;
    if (el.closest('[aria-hidden="true"]')) return true;
    // sr-only: present for assistive tech, clipped to nothing on screen.
    const rect = el.getBoundingClientRect();
    return rect.width <= 1 || rect.height <= 1;
  };

  /** The colour behind `el`, compositing every translucent layer above it. */
  const backgroundOf = (el) => {
    const layers = [];
    for (let node = el; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (style.backgroundImage !== 'none') return null; // a gradient is not one colour
      const bg = style.backgroundColor;
      const alpha = alphaOf(bg);
      if (alpha === 0) continue;
      layers.unshift([bg, alpha]);
      if (alpha === 1) {
        let acc = parseColor(bg);
        for (const [color, a] of layers.slice(1)) acc = over(parseColor(color), acc, a);
        return acc;
      }
    }
    return null;
  };

  const describe = (el) => {
    const slot = el.closest('[data-slot]')?.dataset.slot;
    const text = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
    return `${slot ? `[${slot}] ` : ''}${el.tagName.toLowerCase()} "${text}"`;
  };

  for (const el of document.querySelectorAll('body *')) {
    const ownText = [...el.childNodes].some(
      (n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim().length > 0
    );
    if (!ownText || hidden(el)) continue;

    const style = getComputedStyle(el);
    const background = backgroundOf(el);
    if (!background) continue;

    const size = parseFloat(style.fontSize);
    const weight = Number(style.fontWeight) || 400;
    const large = size >= 24 || (size >= 18.66 && weight >= 700);
    const min = large ? 3 : 4.5;

    const fgAlpha = alphaOf(style.color);
    const foreground =
      fgAlpha === 1 ? parseColor(style.color) : over(parseColor(style.color), background, fgAlpha);

    results.push({
      what: describe(el),
      ratio: contrast(foreground, background),
      min,
      size,
      fg: style.color,
      bg: style.backgroundColor
    });
  }

  return results;
};

// Overlays only render once opened, and an open one covers the page, so each is
// measured on its own pass and dismissed before the next.
const OVERLAYS = ['Popover', 'Menu', 'Tooltip', 'Dialog', 'Sheet', 'Confirm'];

async function openOverlay(page, name) {
  const trigger = page.getByRole('button', { name, exact: true }).first();
  if ((await trigger.count()) === 0) return false;
  if (name === 'Tooltip') await trigger.hover();
  else await trigger.click();
  await page.waitForTimeout(400);
  return true;
}

async function closeOverlay(page) {
  await page.keyboard.press('Escape');
  await page.mouse.move(0, 0);
  await page.waitForTimeout(300);
}

await waitForServer();

const browser = await chromium.launch({ executablePath });
const page = await browser.newPage({ viewport: { width: 1280, height: 1600 } });

const pageErrors = [];
page.on('pageerror', (err) => pageErrors.push(`uncaught: ${err.message}`));
page.on('console', (msg) => {
  if (msg.type() === 'error') pageErrors.push(`console: ${msg.text()}`);
});

const failures = [];
let measured = 0;

for (const theme of ['light', 'dark']) {
  await page.goto(URL_, { waitUntil: 'networkidle' });
  await page.evaluate((t) => localStorage.setItem('prx-theme', t), theme);
  await page.reload({ waitUntil: 'networkidle' });

  const isDark = await page.evaluate(() => document.documentElement.classList.contains('dark'));
  if (isDark !== (theme === 'dark')) failures.push({ theme, what: 'theme class not applied' });

  await page.addScriptTag({ content: colourSource });

  for (const overlay of [null, ...OVERLAYS]) {
    if (overlay && !(await openOverlay(page, overlay))) continue;

    const results = await page.evaluate(MEASURE);
    measured += results.length;
    for (const row of results) {
      if (row.ratio < row.min) failures.push({ theme, overlay, ...row });
    }
    if (process.argv.includes('--verbose')) {
      console.log(`${theme} ${overlay ?? 'page'}: ${results.length} text runs measured`);
    }

    if (overlay) await closeOverlay(page);
  }
}

await browser.close();
stop();

if (pageErrors.length > 0) {
  console.error('\nThe styleguide logged errors:\n');
  for (const e of pageErrors) console.error(`  ${e}`);
  process.exit(1);
}

if (failures.length > 0) {
  console.error(`\nRendered contrast check failed — ${failures.length} element(s) below AA:\n`);
  for (const f of failures) {
    console.error(
      `  ${String(f.theme).padEnd(5)} ${f.overlay ?? 'page'} ${
        f.ratio ? f.ratio.toFixed(2) : '?'
      }:1 < ${f.min}:1  ${f.what}  (${f.fg} on ${f.bg}, ${f.size}px)`
    );
  }
  process.exit(1);
}

console.log(
  `Rendered contrast check passed — ${measured} text runs at WCAG AA across light and dark.`
);
process.exit(0);
