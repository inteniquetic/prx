export type LbStrategy = 'round_robin' | 'random' | 'hash';

export interface TlsConfig {
  listen: string;
  cert_path: string;
  key_path: string;
  enable_h2: boolean;
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

export interface RouteConfig {
  name: string;
  service: string;
  host: string;
  path_prefix: string;
  methods: string[];
  is_default: boolean;
}

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

export const createDefaultRoute = (idx: number, serviceName?: string): RouteConfig => ({
  name: `route-${idx}`,
  service: serviceName ?? 'service-1',
  host: '',
  path_prefix: '/',
  methods: [],
  is_default: idx === 1
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