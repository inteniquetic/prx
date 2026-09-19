// The config-draft endpoints, mocked for the page checks (T307, T308).
//
// Every page loads the draft now — the topbar says whether one is waiting and
// the command palette can open it — so every check has to answer these three,
// or the page logs a failed request and the check fails on it. The checks that
// are about the draft itself (`config:check`, `settings:check`) mock these
// themselves, with state; this is the quiet version for everyone else.

/**
 * @param context Playwright browser context.
 * @param options.toml What `GET /web/config` returns as text.
 * @param options.config What a validation reports as the parsed config.
 */
export async function mockDraftEndpoints(context, { toml, config }) {
  const etag = '"mock-1"';

  await context.route('**/web/config/validate', (route) =>
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        valid: true,
        errors: [],
        warnings: [],
        config: typeof config === 'function' ? config() : config,
        current_etag: etag
      })
    })
  );

  await context.route('**/web/config/edit', (route) =>
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        toml: typeof toml === 'function' ? toml() : toml,
        valid: true,
        errors: [],
        warnings: [],
        config: typeof config === 'function' ? config() : config,
        current_etag: etag
      })
    })
  );

  // The TLS page asks what the running proxy is serving.
  await context.route('**/web/tls/status', (route) =>
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        acme: {
          enabled: false,
          staging: false,
          directory_url: '',
          domains: [],
          last_attempt_epoch_s: null,
          last_success_epoch_s: null,
          last_error: null,
          certificate_expiry_epoch_s: null
        },
        certificates: []
      })
    })
  );

  // The text form of the file, which is what the editor and the diff work on.
  // `**/web/config*` in each check still answers the JSON form.
  await context.route('**/web/config', (route) => {
    const request = route.request();
    if (request.method() === 'HEAD') {
      return route.fulfill({ status: 200, headers: { etag }, body: '' });
    }
    if (new URL(request.url()).searchParams.get('format') === 'json') {
      return route.fallback();
    }
    return route.fulfill({
      contentType: 'text/plain; charset=utf-8',
      headers: { etag },
      body: typeof toml === 'function' ? toml() : toml
    });
  });
}
