export type LbStrategy = 'round_robin' | 'random' | 'hash';

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

export interface ServiceConfig {
  name: string;
  lb: LbStrategy;
  max_retries: number;
  retry_backoff_ms: number;
  circuit_breaker: CircuitBreakerConfig;
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

export const createDefaultService = (idx: number): ServiceConfig => ({
  name: `service-${idx}`,
  lb: 'round_robin',
  max_retries: 0,
  retry_backoff_ms: 0,
  circuit_breaker: createDefaultCircuitBreaker(),
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