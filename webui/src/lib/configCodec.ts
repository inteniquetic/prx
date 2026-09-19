import {
  createDefaultConcurrencyLimit,
  createDefaultHealthCheck,
  createDefaultService,
  createDefaultSticky,
  createDefaultRateLimit,
  createDefaultRouteCache,
  type HeaderRules,
  type PrxConfig,
  type RouteConfig,
  type ServiceConfig,
  type UpstreamConfig
} from './types/config';

const esc = (value: string): string => value.replaceAll('\\', '\\\\').replaceAll('"', '\\"');

const toTomlBool = (value: boolean): string => (value ? 'true' : 'false');

const formatArray = (items: string[]): string => `[${items.map((x) => `"${esc(x)}"`).join(', ')}]`;

const pushOptionalNumber = (lines: string[], key: string, value: number | null) => {
  if (value !== null && Number.isFinite(value)) {
    lines.push(`${key} = ${Math.max(0, Math.floor(value))}`);
  }
};

const renderUpstream = (upstream: UpstreamConfig, prefix: 'service' | 'route' = 'service'): string => {
  const lines = [`[[${prefix}.upstream]]`];
  lines.push(`addr = "${esc(upstream.addr)}"`);
  if (!upstream.enabled) {
    // Drained: written out so the state survives a reload, unlike a weight of
    // 0, which the balancer clamps back to 1.
    lines.push('enabled = false');
  }
  lines.push(`tls = ${toTomlBool(upstream.tls)}`);
  if (upstream.sni.trim()) {
    lines.push(`sni = "${esc(upstream.sni)}"`);
  }
  lines.push(`weight = ${Math.min(256, Math.max(1, upstream.weight))}`);
  if (upstream.verify_cert !== null) {
    lines.push(`verify_cert = ${toTomlBool(upstream.verify_cert)}`);
  }
  if (upstream.verify_hostname !== null) {
    lines.push(`verify_hostname = ${toTomlBool(upstream.verify_hostname)}`);
  }
  pushOptionalNumber(lines, 'connect_timeout_ms', upstream.connect_timeout_ms);
  pushOptionalNumber(
    lines,
    'total_connect_timeout_ms',
    upstream.total_connect_timeout_ms
  );
  pushOptionalNumber(lines, 'read_timeout_ms', upstream.read_timeout_ms);
  pushOptionalNumber(lines, 'write_timeout_ms', upstream.write_timeout_ms);
  pushOptionalNumber(lines, 'idle_timeout_ms', upstream.idle_timeout_ms);
  return lines.join('\n');
};

/**
 * Only settings that differ from prx's own defaults are written, same as for
 * routes: the file stays the size of what someone decided, and a default that
 * moves in a later release still reaches services that never expressed a view.
 */
const renderService = (service: ServiceConfig): string => {
  const defaults = createDefaultService(1);
  const lines = ['[[service]]'];
  lines.push(`name = "${esc(service.name)}"`);
  lines.push(`lb = "${service.lb}"`);
  lines.push(`max_retries = ${Math.max(0, service.max_retries)}`);
  lines.push(`retry_backoff_ms = ${Math.max(0, service.retry_backoff_ms)}`);

  if (service.retry_budget_ratio !== defaults.retry_budget_ratio) {
    lines.push(`retry_budget_ratio = ${service.retry_budget_ratio}`);
  }
  if (service.retry_budget_min_per_window !== defaults.retry_budget_min_per_window) {
    lines.push(`retry_budget_min_per_window = ${service.retry_budget_min_per_window}`);
  }
  if (service.retry_budget_window_ms !== defaults.retry_budget_window_ms) {
    lines.push(`retry_budget_window_ms = ${service.retry_budget_window_ms}`);
  }
  if (service.retry_idempotent_only !== defaults.retry_idempotent_only) {
    lines.push(`retry_idempotent_only = ${toTomlBool(service.retry_idempotent_only)}`);
  }
  if (service.request_timeout_ms !== defaults.request_timeout_ms) {
    lines.push(`request_timeout_ms = ${Math.max(0, service.request_timeout_ms)}`);
  }
  if (service.upstream_h2 !== defaults.upstream_h2) {
    lines.push(`upstream_h2 = "${service.upstream_h2}"`);
  }

  lines.push('');
  lines.push('[service.circuit_breaker]');
  lines.push(`enabled = ${toTomlBool(service.circuit_breaker.enabled)}`);
  lines.push(
    `consecutive_failures = ${Math.max(1, service.circuit_breaker.consecutive_failures)}`
  );
  lines.push(`open_ms = ${Math.max(1, service.circuit_breaker.open_ms)}`);

  const healthDefaults = createDefaultHealthCheck();
  if (service.health_check.enabled) {
    lines.push('');
    lines.push('[service.health_check]');
    lines.push('enabled = true');
    lines.push(`kind = "${service.health_check.kind}"`);
    if (service.health_check.kind === 'http') {
      lines.push(`path = "${esc(service.health_check.path)}"`);
      if (
        service.health_check.expected_status.join(',') !==
        healthDefaults.expected_status.join(',')
      ) {
        lines.push(`expected_status = [${service.health_check.expected_status.join(', ')}]`);
      }
    }
    if (service.health_check.interval_ms !== healthDefaults.interval_ms) {
      lines.push(`interval_ms = ${service.health_check.interval_ms}`);
    }
    if (service.health_check.timeout_ms !== healthDefaults.timeout_ms) {
      lines.push(`timeout_ms = ${service.health_check.timeout_ms}`);
    }
    if (service.health_check.healthy_threshold !== healthDefaults.healthy_threshold) {
      lines.push(`healthy_threshold = ${service.health_check.healthy_threshold}`);
    }
    if (service.health_check.unhealthy_threshold !== healthDefaults.unhealthy_threshold) {
      lines.push(`unhealthy_threshold = ${service.health_check.unhealthy_threshold}`);
    }
  }

  const stickyDefaults = createDefaultSticky();
  if (service.sticky.enabled) {
    lines.push('');
    lines.push('[service.sticky]');
    lines.push('enabled = true');
    lines.push(`mode = "${service.sticky.mode}"`);
    if (service.sticky.name !== stickyDefaults.name) {
      lines.push(`name = "${esc(service.sticky.name)}"`);
    }
    if (service.sticky.mode === 'cookie' && service.sticky.ttl_s !== stickyDefaults.ttl_s) {
      lines.push(`ttl_s = ${service.sticky.ttl_s}`);
    }
  }

  lines.push('');
  const upstreams = service.upstreams.map((u) => renderUpstream(u, 'service')).join('\n\n');
  return `${lines.join('\n')}${upstreams}`;
};

const formatNumberArray = (items: number[]): string => `[${items.join(', ')}]`;

const renderHeaderRules = (
  rules: HeaderRules,
  table: 'request_headers' | 'response_headers'
): string[] => {
  const lines: string[] = [];

  // The parent table comes first: in TOML every key after a table header
  // belongs to that header, so `remove` written after `[...set]` would land in
  // the wrong table.
  if (rules.remove.length > 0) {
    lines.push('');
    lines.push(`[route.${table}]`);
    lines.push(`remove = ${formatArray(rules.remove)}`);
  }

  for (const [kind, map] of [
    ['set', rules.set],
    ['add', rules.add]
  ] as const) {
    const entries = Object.entries(map);
    if (entries.length === 0) continue;
    lines.push('');
    lines.push(`[route.${table}.${kind}]`);
    for (const [name, value] of entries) {
      lines.push(`"${esc(name)}" = "${esc(value)}"`);
    }
  }

  return lines;
};

/**
 * Only settings that differ from the proxy's own defaults are written out.
 *
 * The file stays the size of what someone actually decided, and a default that
 * changes in a later prx release still reaches routes that never expressed an
 * opinion about it.
 */
const renderRoute = (route: RouteConfig): string => {
  const lines = ['[[route]]'];
  lines.push(`name = "${esc(route.name)}"`);
  lines.push(`service = "${esc(route.service)}"`);
  if (route.host.trim()) {
    lines.push(`host = "${esc(route.host)}"`);
  }
  lines.push(`path_prefix = "${esc(route.path_prefix || '/')}"`);
  lines.push(`is_default = ${toTomlBool(route.is_default)}`);
  if (route.methods.length > 0) {
    lines.push(`methods = ${formatArray(route.methods)}`);
  }
  if (!route.enabled) {
    lines.push('enabled = false');
  }

  lines.push(...renderHeaderRules(route.request_headers, 'request_headers'));
  lines.push(...renderHeaderRules(route.response_headers, 'response_headers'));

  const rateDefaults = createDefaultRateLimit();
  if (route.rate_limit.enabled) {
    lines.push('');
    lines.push('[route.rate_limit]');
    lines.push('enabled = true');
    lines.push(`key = "${esc(route.rate_limit.key)}"`);
    lines.push(`requests_per_second = ${Math.max(1, route.rate_limit.requests_per_second)}`);
    if (route.rate_limit.burst > 0) {
      lines.push(`burst = ${route.rate_limit.burst}`);
    }
    if (route.rate_limit.response_status !== rateDefaults.response_status) {
      lines.push(`response_status = ${route.rate_limit.response_status}`);
    }
    if (route.rate_limit.retry_after !== rateDefaults.retry_after) {
      lines.push(`retry_after = ${toTomlBool(route.rate_limit.retry_after)}`);
    }
    if (route.rate_limit.entry_ttl_ms !== rateDefaults.entry_ttl_ms) {
      lines.push(`entry_ttl_ms = ${route.rate_limit.entry_ttl_ms}`);
    }
    if (route.rate_limit.max_entries !== rateDefaults.max_entries) {
      lines.push(`max_entries = ${route.rate_limit.max_entries}`);
    }
  }

  const concurrencyDefaults = createDefaultConcurrencyLimit();
  if (
    route.concurrency_limit.max_concurrent > 0 ||
    route.concurrency_limit.response_status !== concurrencyDefaults.response_status
  ) {
    lines.push('');
    lines.push('[route.concurrency_limit]');
    lines.push(`max_concurrent = ${Math.max(0, route.concurrency_limit.max_concurrent)}`);
    lines.push(`response_status = ${route.concurrency_limit.response_status}`);
  }

  const cacheDefaults = createDefaultRouteCache();
  if (route.cache.enabled) {
    lines.push('');
    lines.push('[route.cache]');
    lines.push('enabled = true');
    lines.push(`ttl_ms = ${Math.max(1, route.cache.ttl_ms)}`);
    if (route.cache.max_body_bytes !== cacheDefaults.max_body_bytes) {
      lines.push(`max_body_bytes = ${route.cache.max_body_bytes}`);
    }
    if (route.cache.max_entries !== cacheDefaults.max_entries) {
      lines.push(`max_entries = ${route.cache.max_entries}`);
    }
    if (route.cache.max_bytes !== cacheDefaults.max_bytes) {
      lines.push(`max_bytes = ${route.cache.max_bytes}`);
    }
    if (
      route.cache.cache_status_codes.join(',') !== cacheDefaults.cache_status_codes.join(',')
    ) {
      lines.push(`cache_status_codes = ${formatNumberArray(route.cache.cache_status_codes)}`);
    }
    if (route.cache.key_query !== cacheDefaults.key_query) {
      lines.push(`key_query = ${toTomlBool(route.cache.key_query)}`);
    }
    if (route.cache.vary_headers.join(',') !== cacheDefaults.vary_headers.join(',')) {
      lines.push(`vary_headers = ${formatArray(route.cache.vary_headers)}`);
    }
    if (route.cache.coalesce_wait_ms !== cacheDefaults.coalesce_wait_ms) {
      lines.push(`coalesce_wait_ms = ${route.cache.coalesce_wait_ms}`);
    }
    if (route.cache.add_status_header !== cacheDefaults.add_status_header) {
      lines.push(`add_status_header = ${toTomlBool(route.cache.add_status_header)}`);
    }
  }

  return lines.join('\n');
};

export const encodeToml = (config: PrxConfig): string => {
  const lines: string[] = [];

  lines.push('[server]');
  lines.push(`listen = ${formatArray(config.server.listen.filter(Boolean))}`);
  lines.push(`health_path = "${esc(config.server.health_path || '/healthz')}"`);
  lines.push(`ready_path = "${esc(config.server.ready_path || '/readyz')}"`);
  pushOptionalNumber(lines, 'threads', config.server.threads);
  pushOptionalNumber(lines, 'grace_period_seconds', config.server.grace_period_seconds);
  pushOptionalNumber(
    lines,
    'graceful_shutdown_timeout_seconds',
    config.server.graceful_shutdown_timeout_seconds
  );
  lines.push(
    `config_reload_debounce_ms = ${Math.max(0, config.server.config_reload_debounce_ms || 0)}`
  );
  if (config.server.tls) {
    lines.push('');
    lines.push('[server.tls]');
    lines.push(`listen = "${esc(config.server.tls.listen)}"`);
    lines.push(`cert_path = "${esc(config.server.tls.cert_path)}"`);
    lines.push(`key_path = "${esc(config.server.tls.key_path)}"`);
    lines.push(`enable_h2 = ${toTomlBool(config.server.tls.enable_h2)}`);
  }
  lines.push('');

  lines.push('[observability]');
  lines.push(`log_level = "${esc(config.observability.log_level || 'info')}"`);
  lines.push(`access_log = ${toTomlBool(config.observability.access_log)}`);
  if (config.observability.prometheus_listen.trim()) {
    lines.push(`prometheus_listen = "${esc(config.observability.prometheus_listen)}"`);
  }

  const serviceBlocks = config.services.map(renderService);
  if (serviceBlocks.length > 0) {
    lines.push('');
    lines.push(serviceBlocks.join('\n\n'));
  }

  const routeBlocks = config.routes.map(renderRoute);
  if (routeBlocks.length > 0) {
    lines.push('');
    lines.push(routeBlocks.join('\n\n'));
  }

  return `${lines.join('\n')}\n`;
};