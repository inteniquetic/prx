import type { components } from './api';

export type LbStrategy = components['schemas']['LbStrategy'];
export type UpstreamH2 = components['schemas']['UpstreamH2'];
export type StickyMode = components['schemas']['StickyMode'];
export type HealthCheckKind = components['schemas']['HealthCheckKind'];
export type HealthCheckConfig = components['schemas']['HealthCheckConfig'];
export type StickyConfig = components['schemas']['StickyConfig'];
export type AcmeConfig = components['schemas']['AdminAcmePayload'];
export type TlsConfig = components['schemas']['AdminTlsPayload'];
export type TlsCertConfig = components['schemas']['AdminTlsCertPayload'];
export type UpstreamConfig = components['schemas']['AdminUpstreamPayload'];
export type CircuitBreakerConfig = components['schemas']['AdminCircuitBreakerPayload'];
export type ServiceConfig = components['schemas']['AdminServicePayload'];
export type HeaderRules = components['schemas']['HeaderRules'];
export type RateLimitConfig = components['schemas']['RateLimitConfig'];
export type ConcurrencyLimitConfig = components['schemas']['ConcurrencyLimitConfig'];
export type RouteConfig = components['schemas']['AdminRoutePayload'];
export type PluginConfig = components['schemas']['AdminPluginPayload'];
export type PrxConfig = components['schemas']['AdminConfigPayload'];
export type RouteCacheConfig = components['schemas']['CacheConfig'];

/** What each strategy does, in the words the form shows next to it. */
export const LB_STRATEGIES: { id: LbStrategy; label: string; description: string }[] = [
  {
    id: 'round_robin',
    label: 'Round robin',
    description: 'Takes each upstream in turn, in proportion to its weight.'
  },
  {
    id: 'random',
    label: 'Random',
    description: 'Picks at random, weighted. Spreads load without any shared state.'
  },
  {
    id: 'hash',
    label: 'Hash',
    description:
      'Hashes the request onto the ring, so the same request keeps landing on the same upstream while the pool is unchanged.'
  },
  {
    id: 'least_conn',
    label: 'Least connections',
    description:
      'Samples two upstreams and keeps the one with fewer requests in flight. Good when requests differ wildly in cost.'
  },
  {
    id: 'p2c_ewma',
    label: 'Least latency',
    description:
      'Samples two upstreams and keeps the faster one by recent latency. Routes around a slow backend before it fails.'
  }
];

export const createDefaultAcme = (): AcmeConfig => ({
  enabled: false,
  email: [],
  directory_url: 'https://acme-v02.api.letsencrypt.org/directory',
  domains: [],
  storage_dir: '/var/lib/prx/acme',
  renew_before_days: 30,
  ca_root_path: null
});

export const createDefaultTls = (): TlsConfig => ({
  listen: '0.0.0.0:8443',
  cert_path: '',
  key_path: '',
  enable_h2: true,
  certs: [],
  acme: createDefaultAcme()
});

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

export const createDefaultUpstream = (): UpstreamConfig => ({
  addr: '127.0.0.1:9000',
  enabled: true,
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

/** Mirrors the serde defaults in `src/config.rs`. */
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
  cache: createDefaultRouteCache(),
  plugins: []
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
    h2c: true,
    tls: null
  },
  observability: {
    log_level: 'info',
    access_log: true,
    prometheus_listen: ''
  },
  services: [createDefaultService(1)],
  routes: [createDefaultRoute(1)],
  plugins: []
});