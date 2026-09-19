// Drives the TOML editor against a mocked admin API (T307 acceptance).
//
// Four claims a type checker cannot see: that a typo is underlined on the right
// line within a second, that CodeMirror is fetched only when this tab is
// opened, that a draft outlives the tab, and that an apply which was overtaken
// by somebody else shows all three sides instead of overwriting them.
//
// Usage: npm run config:check [--verbose]
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

const PORT = Number(process.env.CONFIG_PORT ?? 5207);
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

// --- the config file the mocked admin API is serving ------------------------

const ORIGINAL = `# prx config
[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "info"
access_log = true

[[service]]
name = "api"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3001"
weight = 1

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
`;

/** What another tab applied while this one was editing. */
const OVERTAKEN = ORIGINAL.replace('weight = 1', 'weight = 9');

let fileText = ORIGINAL;
let etagCounter = 1;
const etags = new Map([[ORIGINAL, '"seed-1"']]);
const etagFor = (text) => {
  if (!etags.has(text)) etags.set(text, `"gen-${(etagCounter += 1)}"`);
  return etags.get(text);
};

const upstream = (addr, weight) => ({
  addr,
  enabled: true,
  tls: false,
  sni: '',
  weight,
  verify_cert: null,
  verify_hostname: null,
  connect_timeout_ms: null,
  total_connect_timeout_ms: null,
  read_timeout_ms: null,
  write_timeout_ms: null,
  idle_timeout_ms: null
});

/** A config payload shaped like the admin API's, reflecting the draft's text. */
const configFor = (text) => ({
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
  observability: { log_level: 'info', access_log: true, prometheus_listen: '' },
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
      upstreams: [upstream('127.0.0.1:3001', Number(/weight = (\d+)/.exec(text)?.[1] ?? 1))]
    }
  ],
  routes: [
    {
      name: 'api',
      service: /service = "([^"]*)"/.exec(text)?.[1] ?? 'api',
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

/**
 * Stands in for `src/validate.rs`: enough of it to answer the two cases the
 * page is driven through, with the same shape and the same line numbers.
 */
function validationFor(text) {
  const lines = text.split('\n');

  const brokenAt = lines.findIndex((line) => /^\s*listen = \[\s*$/.test(line));
  if (brokenAt >= 0) {
    return {
      valid: false,
      errors: [
        {
          severity: 'error',
          code: 'invalid_toml',
          path: '',
          message: 'invalid array',
          line: brokenAt + 1,
          column: 10,
          end_line: brokenAt + 1,
          end_column: 11
        }
      ],
      warnings: [],
      current_etag: etagFor(fileText)
    };
  }

  const unknownServiceAt = lines.findIndex((line) => /^service = "missing"/.test(line));
  if (unknownServiceAt >= 0) {
    return {
      valid: false,
      errors: [
        {
          severity: 'error',
          code: 'unknown_service',
          path: 'route[0].service',
          message: "route 'api' references unknown service 'missing'",
          hint: 'services in this config: api',
          line: unknownServiceAt + 1,
          column: 1,
          end_line: unknownServiceAt + 1,
          end_column: 8
        }
      ],
      warnings: [],
      current_etag: etagFor(fileText)
    };
  }

  return {
    valid: true,
    errors: [],
    warnings: [
      {
        severity: 'warning',
        code: 'unused_service',
        path: 'service[0]',
        message: "service 'api' is not used by any route",
        line: 12,
        column: 1,
        end_line: 12,
        end_column: 12
      }
    ],
    config: configFor(text),
    current_etag: etagFor(fileText)
  };
}

const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
  if (process.argv.includes('--verbose')) console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
};

await waitForServer();

const browser = await chromium.launch({ executablePath });
const context = await browser.newContext({ viewport: { width: 1500, height: 1100 } });

/** Every asset the page asked for, to prove what is and is not lazy. */
const requested = [];
context.on('request', (request) => requested.push(request.url()));

let applyCount = 0;

await context.route('**/web/config/validate', (route) =>
  route.fulfill({
    contentType: 'application/json',
    body: JSON.stringify(validationFor(route.request().postData() ?? ''))
  })
);

await context.route('**/web/config*', (route) => {
  const request = route.request();

  if (request.method() === 'HEAD') {
    return route.fulfill({ status: 200, headers: { etag: etagFor(fileText) }, body: '' });
  }

  if (request.method() === 'PUT') {
    applyCount += 1;
    const ifMatch = request.headers()['if-match'];
    if (ifMatch && ifMatch !== etagFor(fileText)) {
      return route.fulfill({
        status: 409,
        contentType: 'application/json',
        body: JSON.stringify({
          error: 'config_changed',
          expected_etag: ifMatch,
          current_etag: etagFor(fileText),
          current_toml: fileText
        })
      });
    }
    fileText = request.postData() ?? '';
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

/** Retypes a whole line in the editor, the way a person would. */
async function replaceLine(matches, replacement) {
  const line = page.locator('.cm-content .cm-line', { hasText: matches }).last();
  await line.click();
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+End');
  await page.keyboard.type(replacement);
}
const editorText = () =>
  page.evaluate(() => {
    // CodeMirror puts each line in its own element, so textContent on the
    // whole editor would run the file together into one line.
    const lines = document.querySelectorAll('[data-slot="toml-editor"] .cm-line');
    if (lines.length > 0) return [...lines].map((line) => line.textContent).join('\n');
    const textarea = document.querySelector('textarea');
    return textarea ? textarea.value : '';
  });

// --- 1. the editor is not in the bundle every other page pays for -----------

await page.goto(`${ORIGIN}/`, { waitUntil: 'networkidle' });
await settle(600);
check(
  !requested.some((url) => /\/editor-[^/]*\.js/.test(url)),
  'the dashboard does not download the editor'
);

await page.goto(`${ORIGIN}/settings`, { waitUntil: 'networkidle' });
await page.getByRole('button', { name: 'TOML', exact: true }).click();
await page.waitForSelector('[data-slot="toml-editor"] .cm-content');
check(
  requested.some((url) => /\/editor-[^/]*\.js/.test(url)),
  'opening the TOML tab downloads it'
);

// --- 2. the file itself is what is on screen, comments and all --------------

const loaded = await editorText();
check(loaded.includes('# prx config'), 'the editor shows the file, comments included');
check(
  loaded.includes('weight = 1'),
  'the editor shows the file as it is on disk, not a re-rendered copy'
);

// --- 3. a mistake is underlined on the right line, quickly -------------------

await replaceLine('service = "api"', 'service = "missing"');

const started = Date.now();
await page.waitForSelector('.cm-lintRange-error', { timeout: 2000 });
const markedIn = Date.now() - started;
check(markedIn < 1000, `the error is marked within a second (took ${markedIn} ms)`);

// The server points at the key it is complaining about, so the mark belongs on
// the `service` key of the line that now names a service that does not exist.
const marked = await page.evaluate(() => {
  const mark = document.querySelector('.cm-lintRange-error');
  const line = mark?.closest('.cm-line');
  return {
    text: mark?.textContent ?? null,
    line: line ? line.textContent : null,
    number: line
      ? [...document.querySelectorAll('.cm-content .cm-line')].indexOf(line) + 1
      : null
  };
});
check(marked.text === 'service', `the underline covers the field (got ${marked.text})`);
check(
  (marked.line ?? '').includes('missing'),
  'and sits on the line that names a service which does not exist'
);
const missingLine =
  (await editorText()).split('\n').findIndex((line) => line.includes('missing')) + 1;
check(
  marked.number === missingLine,
  `on the line the API reported (mark on ${marked.number}, text on ${missingLine})`
);
check(
  (await page.locator('section', { hasText: 'Problems' }).innerText()).includes(
    'unknown service'
  ),
  'the problem is also listed under the editor, in words'
);
check(
  await page.getByRole('button', { name: /Review & apply/ }).isDisabled(),
  'a config with an error cannot be applied'
);

// --- 4. the draft survives the tab being closed -----------------------------

await settle(600);
await page.reload({ waitUntil: 'networkidle' });
await page.getByRole('button', { name: 'TOML', exact: true }).click();
await page.waitForSelector('[data-slot="toml-editor"] .cm-content');
await settle(700);
check(
  (await editorText()).includes('missing'),
  'the draft is still there after the tab was closed and reopened'
);
check(
  (await page.locator('body').innerText()).includes('Draft restored'),
  'and the page says it is a restored draft, not what the proxy is running'
);

// --- 5. the diff and the summary, before anything is written ----------------

// Put the draft back to something valid, but different.
await replaceLine('service = "missing"', 'service = "api"');
await settle(900);
await replaceLine('weight = 1', 'weight = 5');
await settle(900);

check(
  !(await page.getByRole('button', { name: /Review & apply/ }).isDisabled()),
  'a valid draft can be reviewed'
);
await page.getByRole('button', { name: /Review & apply/ }).click();
await page.waitForSelector('[aria-label="Review config changes"]');

const review = page.locator('[aria-label="Review config changes"]');
check((await review.innerText()).includes('@@'), 'the review shows a unified diff');
check(
  (await review.innerText()).includes('weight 1 → 5'),
  'and says what the change means, not just which lines moved'
);
check(
  (await review.locator('.bg-success\\/10').count()) > 0,
  'added lines are marked as added'
);

await review.getByRole('button', { name: 'Side by side' }).click();
await settle(200);
check(
  (await review.innerText()).includes('Running config') ||
    (await review.innerText()).includes('Draft'),
  'the side-by-side view labels which column is which'
);
await review.getByRole('button', { name: 'Unified' }).click();

// --- 6. an apply that was overtaken shows all three sides -------------------

// Somebody else applies something while the review is open.
fileText = OVERTAKEN;

const appliesBefore = applyCount;
await review.getByRole('button', { name: /Apply to the proxy/ }).click();
await page.waitForSelector('text=The config changed while you were editing');
check(applyCount === appliesBefore + 1, 'the apply was attempted');
check(fileText === OVERTAKEN, 'and it did not overwrite the other change');

const conflict = page.locator('[role="dialog"]', { hasText: 'The config changed while you were' });
check(
  (await conflict.innerText()).includes('What you started from'),
  'the conflict shows what this tab started from'
);
await conflict.getByRole('button', { name: 'Your change' }).click();
await settle(200);
check((await conflict.innerText()).includes('Your draft'), 'and what this tab would write');
await conflict.getByRole('button', { name: 'What Apply would do' }).click();
await settle(200);
check(
  (await conflict.innerText()).includes('Running now'),
  'and the result of going ahead anyway'
);

await conflict.getByRole('button', { name: 'Apply mine over theirs' }).click();
await page.waitForSelector('text=Config applied', { timeout: 10_000 });
check(fileText.includes('weight = 5'), 'applying after a rebase writes the draft');
check(
  !(await page.locator('body').innerText()).includes('Draft restored'),
  'and the stored draft is cleared once it has been applied'
);

// --- 7. nothing threw along the way -----------------------------------------

check(pageErrors.length === 0, `no uncaught errors (${pageErrors.join(' | ')})`);

await browser.close();
stop();

if (failures.length > 0) {
  console.error(`\nconfig editor check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  FAIL ${failure}`);
  process.exit(1);
}

console.log('config editor check passed');
