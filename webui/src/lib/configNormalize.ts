import {
  createDefaultCircuitBreaker,
  createDefaultConcurrencyLimit,
  createDefaultConfig,
  createDefaultHeaderRules,
  createDefaultRateLimit,
  createDefaultRoute,
  createDefaultRouteCache,
  createDefaultService,
  createDefaultUpstream,
  type ConcurrencyLimitConfig,
  type HeaderRules,
  type LbStrategy,
  type PrxConfig,
  type RateLimitConfig,
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

const normalizeLb = (value: unknown): LbStrategy => {
  if (value === 'random' || value === 'hash' || value === 'round_robin') {
    return value;
  }
  return 'round_robin';
};

const normalizeUpstream = (upstream: PartialUpstream): UpstreamConfig => {
  const defaults = createDefaultUpstream();
  const weight = parseNullableNumber(upstream.weight) ?? defaults.weight;

  return {
    ...defaults,
    ...upstream,
    addr: String(upstream.addr ?? defaults.addr),
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

  return {
    ...defaults,
    ...service,
    name: String(service.name ?? defaults.name),
    lb: normalizeLb(service.lb),
    max_retries: parseNullableNumber(service.max_retries) ?? defaults.max_retries,
    retry_backoff_ms: parseNullableNumber(service.retry_backoff_ms) ?? defaults.retry_backoff_ms,
    circuit_breaker: {
      ...createDefaultCircuitBreaker(),
      ...(service.circuit_breaker ?? {})
    },
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

export const normalizePrxConfig = (input: ConfigInput): PrxConfig => {
  const defaults = createDefaultConfig();
  const serviceSource =
    Array.isArray(input.services) && input.services.length > 0
      ? input.services
      : Array.isArray(input.service) && input.service.length > 0
        ? input.service
        : defaults.services;

  const routeSource =
    Array.isArray(input.routes) && input.routes.length > 0
      ? input.routes
      : Array.isArray(input.route) && input.route.length > 0
        ? input.route
        : defaults.routes;

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
      tls:
        input.server?.tls === null
          ? null
          : input.server?.tls
            ? {
                listen: String(input.server.tls.listen ?? '0.0.0.0:8443'),
                cert_path: String(input.server.tls.cert_path ?? ''),
                key_path: String(input.server.tls.key_path ?? ''),
                enable_h2: input.server.tls.enable_h2 ?? true
              }
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