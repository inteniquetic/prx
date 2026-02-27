import {
  createDefaultCircuitBreaker,
  createDefaultConfig,
  createDefaultRoute,
  createDefaultUpstream,
  type LbStrategy,
  type PrxConfig,
  type RouteConfig,
  type UpstreamConfig
} from './types/config';

type PartialUpstream = Partial<UpstreamConfig>;

type PartialRoute = Partial<RouteConfig> & {
  upstream?: PartialUpstream[];
  upstreams?: PartialUpstream[];
  host?: string | null;
};

type PartialObservability = Partial<PrxConfig['observability']> & {
  prometheus_listen?: string | null;
};

type ConfigInput = Partial<PrxConfig> & {
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

const normalizeRoute = (route: PartialRoute, routeIndex: number): RouteConfig => {
  const defaults = createDefaultRoute(routeIndex + 1);
  const upstreamSource =
    Array.isArray(route.upstreams) && route.upstreams.length > 0
      ? route.upstreams
      : Array.isArray(route.upstream) && route.upstream.length > 0
        ? route.upstream
        : [createDefaultUpstream()];

  return {
    ...defaults,
    ...route,
    name: String(route.name ?? defaults.name),
    host: route.host == null ? '' : String(route.host),
    path_prefix: String(route.path_prefix ?? defaults.path_prefix),
    is_default: route.is_default ?? defaults.is_default,
    lb: normalizeLb(route.lb),
    max_retries: parseNullableNumber(route.max_retries) ?? defaults.max_retries,
    retry_backoff_ms: parseNullableNumber(route.retry_backoff_ms) ?? defaults.retry_backoff_ms,
    circuit_breaker: {
      ...createDefaultCircuitBreaker(),
      ...(route.circuit_breaker ?? {})
    },
    upstreams: upstreamSource.map(normalizeUpstream)
  };
};

export const normalizePrxConfig = (input: ConfigInput): PrxConfig => {
  const defaults = createDefaultConfig();
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
    routes: routeSource.map(normalizeRoute)
  };
};
