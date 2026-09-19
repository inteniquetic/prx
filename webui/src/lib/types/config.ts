export type LbStrategy = 'round_robin' | 'random' | 'hash' | 'least_conn' | 'p2c_ewma';

export const LB_STRATEGIES: LbStrategy[] = [
  'round_robin',
  'random',
  'hash',
  'least_conn',
  'p2c_ewma'
];

/** Which HTTP version prx speaks to a service's upstreams. */
export type UpstreamH2 = 'never' | 'always' | 'auto';

export interface AcmeConfig {
  enabled: boolean;
  email: string[];
  directory_url: string;
  domains: string[];
  storage_dir: string;
  renew_before_days: number;
  ca_root_path: string | null;
}

export interface TlsConfig {
  listen: string;
  cert_path: string;
  key_path: string;
  enable_h2: boolean;
  acme?: AcmeConfig;
}

export interface UpstreamConfig {
  addr: string;
  tls: boolean;
  sni: string;
  weight: number;
  verify_cert: boolean | null;
  verify_hostname: boolean | null;
  connect_timeout_ms: number | null;
  total_connect_timeout_ms: number | null;
  read_timeout_ms: number | null;
  write_timeout_ms: number | null;
  idle_timeout_ms: number | null;
}

export interface CircuitBreakerConfig {
  enabled: boolean;
  consecutive_failures: number;
  open_ms: number;
}

/** `[service.health_check]`: background probing of the upstreams. */
export interface HealthCheckConfig {
  enabled: boolean;
  kind: 'tcp' | 'http';
  path: string;
  interval_ms: number;
  timeout_ms: number;
  healthy_threshold: number;
  unhealthy_threshold: number;
  expected_status: number[];
}

/** `[service.sticky]`: session affinity. */
export interface StickyConfig {
  enabled: boolean;
  mode: 'cookie' | 'client_ip' | 'header';
  /** Cookie or header name, depending on the mode. */
  name: string;
  ttl_s: number;
}

export interface ServiceConfig {
  name: string;
  lb: LbStrategy;
  max_retries: number;
  retry_backoff_ms: number;
  /** `0` disables the retry budget. */
  retry_budget_ratio: number;
  retry_budget_min_per_window: number;
  retry_budget_window_ms: number;
  retry_idempotent_only: boolean;
  /** Total time for a request including retries; `0` disables it. */
  request_timeout_ms: number;
  upstream_h2: UpstreamH2;
  circuit_breaker: CircuitBreakerConfig;
  health_check: HealthCheckConfig;
  sticky: StickyConfig;
  upstreams: UpstreamConfig[];
}

/** `[route.request_headers]` / `[route.response_headers]`. */
export interface HeaderRules {
  set: Record<string, string>;
  add: Record<string, string>;
  remove: string[];
}

export interface RateLimitConfig {
  enabled: boolean;
  /** `client_ip`, `route`, or `header:<Name>`. */
  key: string;
  requests_per_second: number;
  burst: number;
  response_status: number;
  retry_after: boolean;
  entry_ttl_ms: number;
  max_entries: number;
}

export interface ConcurrencyLimitConfig {
  /** `0` means unlimited. */
  max_concurrent: number;
  response_status: number;
}

export interface RouteCacheConfig {
  enabled: boolean;
  ttl_ms: number;
  max_body_bytes: number;
  max_entries: number;
  max_bytes: number;
  cache_status_codes: number[];
  key_query: boolean;
  vary_headers: string[];
  coalesce_wait_ms: number;
  add_status_header: boolean;
}

export interface RouteConfig {
  name: string;
  service: string;
  host: string;
  path_prefix: string;
  methods: string[];
  is_default: boolean;
  /** `false` parks the route: it stays in the config but never matches. */
  enabled: boolean;
  request_headers: HeaderRules;
  response_headers: HeaderRules;
  rate_limit: RateLimitConfig;
  concurrency_limit: ConcurrencyLimitConfig;
  cache: RouteCacheConfig;
}

export const HTTP_METHODS = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
  'HEAD',
  'OPTIONS',
  'TRACE',
  'CONNECT'
] as const;

export type HttpMethod = (typeof HTTP_METHODS)[number];

export interface PrxConfig {
  server: {
    listen: string[];
    health_path: string;
    ready_path: string;
    threads: number | null;
    grace_period_seconds: number | null;
    graceful_shutdown_timeout_seconds: number | null;
    config_reload_debounce_ms: number;
    tls: TlsConfig | null;
  };
  observability: {
    log_level: string;
    access_log: boolean;
    prometheus_listen: string;
  };
  services: ServiceConfig[];
  routes: RouteConfig[];
}

export const createDefaultUpstream = (): UpstreamConfig => ({
  addr: '127.0.0.1:9000',
  tls: false,
  sni: 'localhost',
  weight: 1,
  verify_cert: null,
  verify_hostname: null,
  connect_timeout_ms: null,
  total_connect_timeout_ms: null,
  read_timeout_ms: null,
  write_timeout_ms: null,
  idle_timeout_ms: null
});

export const createDefaultCircuitBreaker = (): CircuitBreakerConfig => ({
  enabled: false,
  consecutive_failures: 3,
  open_ms: 30000
});

/** Mirrors the serde defaults in `src/config.rs`, like the route defaults below. */
export const createDefaultHealthCheck = (): HealthCheckConfig => ({
  enabled: false,
  kind: 'tcp',
  path: '/healthz',
  interval_ms: 2000,
  timeout_ms: 1000,
  healthy_threshold: 2,
  unhealthy_threshold: 3,
  expected_status: [200]
});

export const createDefaultSticky = (): StickyConfig => ({
  enabled: false,
  mode: 'cookie',
  name: 'prx_upstream',
  ttl_s: 3600
});

export const createDefaultService = (idx: number): ServiceConfig => ({
  name: `service-${idx}`,
  lb: 'round_robin',
  max_retries: 0,
  retry_backoff_ms: 0,
  retry_budget_ratio: 0.2,
  retry_budget_min_per_window: 10,
  retry_budget_window_ms: 10000,
  retry_idempotent_only: true,
  request_timeout_ms: 0,
  upstream_h2: 'never',
  circuit_breaker: createDefaultCircuitBreaker(),
  health_check: createDefaultHealthCheck(),
  sticky: createDefaultSticky(),
  upstreams: [createDefaultUpstream()]
});

export const createDefaultHeaderRules = (): HeaderRules => ({
  set: {},
  add: {},
  remove: []
});

/** Mirrors the serde defaults in `src/config.rs`, so a route created here and
 *  one created by the proxy start from the same place. */
export const createDefaultRateLimit = (): RateLimitConfig => ({
  enabled: false,
  key: 'client_ip',
  requests_per_second: 100,
  burst: 0,
  response_status: 429,
  retry_after: true,
  entry_ttl_ms: 60000,
  max_entries: 100000
});

export const createDefaultConcurrencyLimit = (): ConcurrencyLimitConfig => ({
  max_concurrent: 0,
  response_status: 503
});

export const createDefaultRouteCache = (): RouteCacheConfig => ({
  enabled: false,
  ttl_ms: 2000,
  max_body_bytes: 256 * 1024,
  max_entries: 10000,
  max_bytes: 128 * 1024 * 1024,
  cache_status_codes: [200, 203, 300, 301, 404],
  key_query: true,
  vary_headers: ['accept-encoding'],
  coalesce_wait_ms: 2000,
  add_status_header: true
});

export const createDefaultRoute = (idx: number, serviceName?: string): RouteConfig => ({
  name: `route-${idx}`,
  service: serviceName ?? 'service-1',
  host: '',
  path_prefix: '/',
  methods: [],
  is_default: idx === 1,
  enabled: true,
  request_headers: createDefaultHeaderRules(),
  response_headers: createDefaultHeaderRules(),
  rate_limit: createDefaultRateLimit(),
  concurrency_limit: createDefaultConcurrencyLimit(),
  cache: createDefaultRouteCache()
});

export const createDefaultConfig = (): PrxConfig => ({
  server: {
    listen: ['0.0.0.0:8080'],
    health_path: '/healthz',
    ready_path: '/readyz',
    threads: null,
    grace_period_seconds: null,
    graceful_shutdown_timeout_seconds: null,
    config_reload_debounce_ms: 250,
    tls: null
  },
  observability: {
    log_level: 'info',
    access_log: true,
    prometheus_listen: ''
  },
  services: [createDefaultService(1)],
  routes: [createDefaultRoute(1)]
});