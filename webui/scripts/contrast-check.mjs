// Fails when a design token pair drops below its WCAG AA threshold.
//
// The palette is the one place a contrast bug is cheap to fix and expensive to
// find later: every screen inherits it. This reads the tokens straight out of
// app.css and checks the pairs the components actually put together, in both
// themes, so "it looked fine in dark mode" cannot ship.
//
// Thresholds (WCAG 2.2 AA):
//   4.5  normal-size text on its background                    (1.4.3)
//   3.0  large text, and the parts of a control that identify  (1.4.11)
//        it or its state — an input's edge, a status marker
//
// Purely decorative edges (a card outline, a divider between rows) are not
// covered by 1.4.11 and are not listed here: nothing is lost if they are
// invisible. Anything that carries meaning is listed.
import { readFile } from 'node:fs/promises';
import { contrast, over, parseColor } from './contrast.mjs';

const CSS = new URL('../src/app.css', import.meta.url);

function readTokens(css, selector) {
  // Take the first block for the selector; the @theme inline block only maps
  // names onto these, so it holds no values of its own.
  const start = css.indexOf(selector + ' {');
  if (start === -1) throw new Error(`no ${selector} block in app.css`);
  const body = css.slice(start, css.indexOf('}', start));
  const tokens = {};
  for (const [, name, value] of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) {
    if (value.includes('oklch') || value.startsWith('#')) tokens[name] = parseColor(value);
  }
  return tokens;
}

// [foreground, background, minimum, what it is]
const TEXT = [
  ['foreground', 'background', 4.5, 'body text'],
  ['card-foreground', 'card', 4.5, 'card text'],
  ['popover-foreground', 'popover', 4.5, 'popover text'],
  ['muted-foreground', 'background', 4.5, 'secondary text on the page'],
  ['muted-foreground', 'card', 4.5, 'secondary text in a card'],
  ['muted-foreground', 'muted', 4.5, 'text on a muted fill'],
  ['primary-foreground', 'primary', 4.5, 'primary button label'],
  ['secondary-foreground', 'secondary', 4.5, 'secondary button label'],
  ['accent-foreground', 'accent', 4.5, 'hovered menu item'],
  ['destructive-foreground', 'destructive', 4.5, 'destructive button label'],
  ['success-foreground', 'success', 4.5, 'success badge label'],
  ['warning-foreground', 'warning', 4.5, 'warning badge label'],
  ['circuit-foreground', 'circuit', 4.5, 'circuit-open badge label'],
  ['tooltip-foreground', 'tooltip', 4.5, 'tooltip text'],
  ['sidebar-foreground', 'sidebar', 4.5, 'sidebar text'],
  ['sidebar-primary-foreground', 'sidebar-primary', 4.5, 'active sidebar item'],
  ['sidebar-accent-foreground', 'sidebar-accent', 4.5, 'hovered sidebar item'],
  // Status and validation wording on the page and inside cards.
  ['success-emphasis', 'background', 4.5, 'healthy text on the page'],
  ['success-emphasis', 'card', 4.5, 'healthy text in a card'],
  ['warning-emphasis', 'background', 4.5, 'degraded text on the page'],
  ['warning-emphasis', 'card', 4.5, 'degraded text in a card'],
  ['destructive-emphasis', 'background', 4.5, 'down/error text on the page'],
  ['destructive-emphasis', 'card', 4.5, 'down/error text in a card'],
  ['circuit-emphasis', 'background', 4.5, 'circuit-open text on the page'],
  ['circuit-emphasis', 'card', 4.5, 'circuit-open text in a card'],
  ['primary', 'background', 4.5, 'link and info text on the page'],
  ['primary', 'card', 4.5, 'link and info text in a card']
];

// Text over a 10% tint of itself — the alert and toast backgrounds.
const TINTED = [
  ['success-emphasis', 'success', 'card', 4.5, 'success alert text'],
  ['warning-emphasis', 'warning', 'card', 4.5, 'warning alert text'],
  ['destructive-emphasis', 'destructive', 'card', 4.5, 'destructive alert text'],
  ['primary', 'primary', 'card', 4.5, 'info alert text']
];

// Non-text: the pixels that say a control is there, or which state it is in.
const UI = [
  ['input', 'background', 3, 'input border on the page'],
  ['input', 'card', 3, 'input border in a card'],
  ['ring', 'background', 3, 'focus ring'],
  ['switch-track', 'card', 3, 'switch, off'],
  ['primary', 'card', 3, 'switch, on'],
  ['success', 'card', 3, 'healthy marker'],
  ['warning', 'card', 3, 'degraded marker'],
  ['destructive', 'card', 3, 'down marker'],
  ['circuit', 'card', 3, 'circuit-open marker'],
  ['muted-foreground', 'card', 3, 'disabled/unknown marker']
];

const css = await readFile(CSS, 'utf8');
const themes = { light: readTokens(css, ':root'), dark: readTokens(css, '.dark') };

const failures = [];
const rows = [];

for (const [theme, tokens] of Object.entries(themes)) {
  const get = (name) => {
    const value = tokens[name];
    if (!value) throw new Error(`token --${name} is missing from the ${theme} theme`);
    return value;
  };

  for (const [fg, bg, min, what] of TEXT) {
    const ratio = contrast(get(fg), get(bg));
    rows.push({ theme, what, pair: `${fg} on ${bg}`, ratio, min });
    if (ratio < min) failures.push({ theme, what, pair: `${fg} on ${bg}`, ratio, min });
  }

  for (const [fg, tint, base, min, what] of TINTED) {
    const ratio = contrast(get(fg), over(get(tint), get(base), 0.1));
    rows.push({ theme, what, pair: `${fg} on ${tint}/10`, ratio, min });
    if (ratio < min) failures.push({ theme, what, pair: `${fg} on ${tint}/10`, ratio, min });
  }

  for (const [fg, bg, min, what] of UI) {
    const ratio = contrast(get(fg), get(bg));
    rows.push({ theme, what, pair: `${fg} on ${bg}`, ratio, min });
    if (ratio < min) failures.push({ theme, what, pair: `${fg} on ${bg}`, ratio, min });
  }
}

if (process.argv.includes('--verbose')) {
  for (const row of rows) {
    console.log(
      `${row.ratio >= row.min ? 'ok  ' : 'FAIL'} ${row.theme.padEnd(5)} ${row.ratio
        .toFixed(2)
        .padStart(5)}:1 (min ${row.min})  ${row.pair} — ${row.what}`
    );
  }
}

if (failures.length > 0) {
  console.error(`\nContrast check failed — ${failures.length} token pair(s) below WCAG AA:\n`);
  for (const f of failures) {
    console.error(
      `  ${f.theme.padEnd(5)} ${f.ratio.toFixed(2)}:1 < ${f.min}:1  ${f.pair} — ${f.what}`
    );
  }
  console.error('\nAdjust the token in src/app.css; do not lower the threshold.\n');
  process.exit(1);
}

console.log(`Contrast check passed — ${rows.length} token pairs at WCAG AA in both themes.`);
