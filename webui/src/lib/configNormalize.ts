import {
  createDefaultCircuitBreaker,
  createDefaultConcurrencyLimit,
  createDefaultHealthCheck,
  createDefaultSticky,
  createDefaultConfig,
  createDefaultHeaderRules,
  createDefaultRateLimit,
  createDefaultRoute,
  createDefaultRouteCache,
  createDefaultService,
  createDefaultTls,
  createDefaultUpstream,
  type ConcurrencyLimitConfig,
  type HeaderRules,
  type HealthCheckConfig,
  type StickyConfig,
  type UpstreamH2,
  type LbStrategy,
  type AcmeConfig,
  type PrxConfig,
  type RateLimitConfig,
  type TlsConfig,
  type RouteCacheConfig,
  type RouteConfig,
  type ServiceConfig,
  type UpstreamConfig
} from './types/config';

type PartialUpstream = Partial<UpstreamConfig>;

type PartialService = Partial<ServiceConfig> & {
  upstream?: PartialUpstream[];
  upstreams?: PartialUpstream[];
  circuit_breaker?: Partial<ServiceConfig['circuit_breaker']>;
};

type PartialRoute = Partial<RouteConfig> & {
  host?: string | null;
  methods?: string[] | null;
};

type PartialObservability = Partial<PrxConfig['observability']> & {
  prometheus_listen?: string | null;
};

type ConfigInput = Partial<PrxConfig> & {
  service?: PartialService[];
  services?: PartialService[];
  route?: PartialRoute[];
  routes?: PartialRoute[];
  observability?: PartialObservability;
};

const parseNullableNumber = (value: unknown): number | null => {
  if (value === null || value === undefined || value === '') {
    return null;
  }

  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return null;
  }

  return Math.max(0, Math.floor(parsed));
};

const asRecord = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};

const asBool = (value: unknown, fallback: boolean): boolean =>
  typeof value === 'boolean' ? value : fallback;

const asNumber = (value: unknown, fallback: number): number => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
};

const asStringMap = (value: unknown): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const [key, entry] of Object.entries(asRecord(value))) {
    if (typeof entry === 'string') out[key] = entry;
  }
  return out;
};

const asStringList = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];

const normalizeHeaderRules = (value: unknown): HeaderRules => {
  const source = asRecord(value);
  const defaults = createDefaultHeaderRules();
  return {
    set: source.set === undefined ? defaults.set : asStringMap(source.set),
    add: source.add === undefined ? defaults.add : asStringMap(source.add),
    remove: source.remove === undefined ? defaults.remove : asStringList(source.remove)
  };
};

const normalizeRateLimit = (value: unknown): RateLimitConfig => {
  const source = asRecord(value);
  const defaults = createDefaultRateLimit();
  return {
    enabled: asBool(source.enabled, defaults.enabled),
    key: typeof source.key === 'string' && source.key ? source.key : defaults.key,
    requests_per_second: asNumber(source.requests_per_second, defaults.requests_per_second),
    burst: asNumber(source.burst, defaults.burst),
    response_status: asNumber(source.response_status, defaults.response_status),
    retry_after: asBool(source.retry_after, defaults.retry_after),
    entry_ttl_ms: asNumber(source.entry_ttl_ms, defaults.entry_ttl_ms),
    max_entries: asNumber(source.max_entries, defaults.max_entries)
  };
};

const normalizeConcurrencyLimit = (value: unknown): ConcurrencyLimitConfig => {
  const source = asRecord(value);
  const defaults = createDefaultConcurrencyLimit();
  return {
    max_concurrent: asNumber(source.max_concurrent, defaults.max_concurrent),
    response_status: asNumber(source.response_status, defaults.response_status)
  };
};

const normalizeRouteCache = (value: unknown): RouteCacheConfig => {
  const source = asRecord(value);
  const defaults = createDefaultRouteCache();
  const statusCodes = Array.isArray(source.cache_status_codes)
    ? source.cache_status_codes.map((code) => Number(code)).filter((code) => Number.isFinite(code))
    : defaults.cache_status_codes;
  return {
    enabled: asBool(source.enabled, defaults.enabled),
    ttl_ms: asNumber(source.ttl_ms, defaults.ttl_ms),
    max_body_bytes: asNumber(source.max_body_bytes, defaults.max_body_bytes),
    max_entries: asNumber(source.max_entries, defaults.max_entries),
    max_bytes: asNumber(source.max_bytes, defaults.max_bytes),
    cache_status_codes: statusCodes,
    key_query: asBool(source.key_query, defaults.key_query),
    vary_headers:
      source.vary_headers === undefined ? defaults.vary_headers : asStringList(source.vary_headers),
    coalesce_wait_ms: asNumber(source.coalesce_wait_ms, defaults.coalesce_wait_ms),
    add_status_header: asBool(source.add_status_header, defaults.add_status_header)
  };
};

const LB_VALUES: LbStrategy[] = ['round_robin', 'random', 'hash', 'least_conn', 'p2c_ewma'];

const normalizeLb = (value: unknown): LbStrategy =>
  LB_VALUES.includes(value as LbStrategy) ? (value as LbStrategy) : 'round_robin';

const normalizeHealthCheck = (value: unknown): HealthCheckConfig => {
  const source = asRecord(value);
  const defaults = createDefaultHealthCheck();
  const expected = Array.isArray(source.expected_status)
    ? source.expected_status.map((code) => Number(code)).filter((code) => Number.isFinite(code))
    : defaults.expected_status;
  return {
    enabled: asBool(source.enabled, defaults.enabled),
    kind: source.kind === 'http' ? 'http' : defaults.kind,
    path: typeof source.path === 'string' && source.path ? source.path : defaults.path,
    interval_ms: asNumber(source.interval_ms, defaults.interval_ms),
    timeout_ms: asNumber(source.timeout_ms, defaults.timeout_ms),
    healthy_threshold: asNumber(source.healthy_threshold, defaults.healthy_threshold),
    unhealthy_threshold: asNumber(source.unhealthy_threshold, defaults.unhealthy_threshold),
    expected_status: expected
  };
};

const normalizeSticky = (value: unknown): StickyConfig => {
  const source = asRecord(value);
  const defaults = createDefaultSticky();
  const mode =
    source.mode === 'client_ip' || source.mode === 'header' || source.mode === 'cookie'
      ? source.mode
      : defaults.mode;
  return {
    enabled: asBool(source.enabled, defaults.enabled),
    mode,
    name: typeof source.name === 'string' && source.name ? source.name : defaults.name,
    ttl_s: asNumber(source.ttl_s, defaults.ttl_s)
  };
};

const normalizeUpstreamH2 = (value: unknown, fallback: UpstreamH2): UpstreamH2 =>
  value === 'always' || value === 'auto' || value === 'never' ? value : fallback;

const normalizeUpstream = (upstream: PartialUpstream): UpstreamConfig => {
  const defaults = createDefaultUpstream();
  const weight = parseNullableNumber(upstream.weight) ?? defaults.weight;

  const source = upstream as Record<string, unknown>;

  return {
    ...defaults,
    ...upstream,
    addr: String(upstream.addr ?? defaults.addr),
    enabled: asBool(source.enabled, defaults.enabled),
    tls: upstream.tls ?? defaults.tls,
    sni: upstream.sni == null ? '' : String(upstream.sni),
    weight: Math.max(1, Math.min(256, weight)),
    verify_cert:
      typeof upstream.verify_cert === 'boolean' ? upstream.verify_cert : defaults.verify_cert,
    verify_hostname:
      typeof upstream.verify_hostname === 'boolean'
        ? upstream.verify_hostname
        : defaults.verify_hostname,
    connect_timeout_ms: parseNullableNumber(upstream.connect_timeout_ms),
    total_connect_timeout_ms: parseNullableNumber(upstream.total_connect_timeout_ms),
    read_timeout_ms: parseNullableNumber(upstream.read_timeout_ms),
    write_timeout_ms: parseNullableNumber(upstream.write_timeout_ms),
    idle_timeout_ms: parseNullableNumber(upstream.idle_timeout_ms)
  };
};

const normalizeService = (service: PartialService, serviceIndex: number): ServiceConfig => {
  const defaults = createDefaultService(serviceIndex + 1);
  const upstreamSource =
    Array.isArray(service.upstreams) && service.upstreams.length > 0
      ? service.upstreams
      : Array.isArray(service.upstream) && service.upstream.length > 0
        ? service.upstream
        : [createDefaultUpstream()];

  const source = service as Record<string, unknown>;

  return {
    ...defaults,
    ...service,
    name: String(service.name ?? defaults.name),
    lb: normalizeLb(service.lb),
    max_retries: parseNullableNumber(service.max_retries) ?? defaults.max_retries,
    retry_backoff_ms: parseNullableNumber(service.retry_backoff_ms) ?? defaults.retry_backoff_ms,
    // Same rule as routes: whatever the config can say about a service travels
    // with it, so reading it and writing it back cannot delete anything.
    retry_budget_ratio: asNumber(source.retry_budget_ratio, defaults.retry_budget_ratio),
    retry_budget_min_per_window: asNumber(
      source.retry_budget_min_per_window,
      defaults.retry_budget_min_per_window
    ),
    retry_budget_window_ms: asNumber(
      source.retry_budget_window_ms,
      defaults.retry_budget_window_ms
    ),
    retry_idempotent_only: asBool(source.retry_idempotent_only, defaults.retry_idempotent_only),
    request_timeout_ms: asNumber(source.request_timeout_ms, defaults.request_timeout_ms),
    upstream_h2: normalizeUpstreamH2(source.upstream_h2, defaults.upstream_h2),
    circuit_breaker: {
      ...createDefaultCircuitBreaker(),
      ...(service.circuit_breaker ?? {})
    },
    health_check: normalizeHealthCheck(source.health_check),
    sticky: normalizeSticky(source.sticky),
    upstreams: upstreamSource.map(normalizeUpstream)
  };
};

const normalizeRoute = (route: PartialRoute, routeIndex: number): RouteConfig => {
  const defaults = createDefaultRoute(routeIndex + 1);
  const methods = Array.isArray(route.methods)
    ? route.methods.filter((m): m is string => typeof m === 'string' && m.trim().length > 0)
    : [];

  const source = route as Record<string, unknown>;

  return {
    ...defaults,
    ...route,
    name: String(route.name ?? defaults.name),
    service: route.service == null ? defaults.service : String(route.service),
    host: route.host == null ? '' : String(route.host),
    path_prefix: String(route.path_prefix ?? defaults.path_prefix),
    methods,
    is_default: route.is_default ?? defaults.is_default,
    // Everything the config can say about a route travels with it. A UI that
    // reads a partial route and writes it back is a UI that quietly deletes
    // header rules, limits and cache settings.
    enabled: asBool(source.enabled, defaults.enabled),
    request_headers: normalizeHeaderRules(source.request_headers),
    response_headers: normalizeHeaderRules(source.response_headers),
    rate_limit: normalizeRateLimit(source.rate_limit),
    concurrency_limit: normalizeConcurrencyLimit(source.concurrency_limit),
    cache: normalizeRouteCache(source.cache)
  };
};

/** `[server.tls]`, including the certificate list and ACME (T308). */
const normalizeTls = (input: Partial<TlsConfig>): TlsConfig => {
  const defaults = createDefaultTls();
  const acme = (input.acme ?? {}) as Partial<AcmeConfig>;

  return {
    listen: String(input.listen ?? defaults.listen),
    cert_path: String(input.cert_path ?? ''),
    key_path: String(input.key_path ?? ''),
    enable_h2: input.enable_h2 ?? defaults.enable_h2,
    certs: (Array.isArray(input.certs) ? input.certs : []).map((cert) => ({
      domains: Array.isArray(cert?.domains) ? cert.domains.map(String) : [],
      cert_path: String(cert?.cert_path ?? ''),
      key_path: String(cert?.key_path ?? ''),
      is_default: cert?.is_default ?? false
    })),
    acme: {
      enabled: acme.enabled ?? false,
      email: Array.isArray(acme.email) ? acme.email.map(String) : [],
      directory_url: String(acme.directory_url ?? defaults.acme.directory_url),
      domains: Array.isArray(acme.domains) ? acme.domains.map(String) : [],
      storage_dir: String(acme.storage_dir ?? defaults.acme.storage_dir),
      renew_before_days: Number(acme.renew_before_days ?? defaults.acme.renew_before_days),
      ca_root_path: acme.ca_root_path == null ? null : String(acme.ca_root_path)
    }
  };
};

export const normalizePrxConfig = (input: ConfigInput): PrxConfig => {
  const defaults = createDefaultConfig();
  // `[[service]]` in the file arrives as `service`; `?format=json` calls the
  // same thing `services`. Either may be absent, and absent is not the same as
  // a starter service: a proxy with nothing in it has to look like one, or the
  // empty states never show and the setup wizard never offers itself (T309).
  const serviceSource =
    (Array.isArray(input.services) ? input.services : undefined) ??
    (Array.isArray(input.service) ? input.service : undefined) ??
    defaults.services;

  const routeSource =
    (Array.isArray(input.routes) ? input.routes : undefined) ??
    (Array.isArray(input.route) ? input.route : undefined) ??
    defaults.routes;

  return {
    server: {
      ...defaults.server,
      ...(input.server ?? {}),
      listen:
        Array.isArray(input.server?.listen) && input.server.listen.length > 0
          ? input.server.listen.map((value) => String(value).trim()).filter(Boolean)
          : defaults.server.listen,
      health_path: String(input.server?.health_path ?? defaults.server.health_path),
      ready_path: String(input.server?.ready_path ?? defaults.server.ready_path),
      threads: parseNullableNumber(input.server?.threads),
      grace_period_seconds: parseNullableNumber(input.server?.grace_period_seconds),
      graceful_shutdown_timeout_seconds: parseNullableNumber(
        input.server?.graceful_shutdown_timeout_seconds
      ),
      config_reload_debounce_ms:
        parseNullableNumber(input.server?.config_reload_debounce_ms) ??
        defaults.server.config_reload_debounce_ms,
      h2c: input.server?.h2c ?? defaults.server.h2c,
      tls:
        input.server?.tls === null
          ? null
          : input.server?.tls
            ? normalizeTls(input.server.tls)
            : defaults.server.tls
    },
    observability: {
      ...defaults.observability,
      ...(input.observability ?? {}),
      log_level: String(input.observability?.log_level ?? defaults.observability.log_level),
      access_log: input.observability?.access_log ?? defaults.observability.access_log,
      prometheus_listen:
        input.observability?.prometheus_listen == null
          ? ''
          : String(input.observability.prometheus_listen)
    },
    services: serviceSource.map(normalizeService),
    routes: routeSource.map(normalizeRoute)
  };
};