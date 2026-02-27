import type { PrxConfig, RouteConfig, UpstreamConfig } from './types/config';

const esc = (value: string): string => value.replaceAll('\\', '\\\\').replaceAll('"', '\\"');

const toTomlBool = (value: boolean): string => (value ? 'true' : 'false');

const formatArray = (items: string[]): string => `[${items.map((x) => `"${esc(x)}"`).join(', ')}]`;

const pushOptionalNumber = (lines: string[], key: string, value: number | null) => {
  if (value !== null && Number.isFinite(value)) {
    lines.push(`${key} = ${Math.max(0, Math.floor(value))}`);
  }
};

const renderUpstream = (upstream: UpstreamConfig): string => {
  const lines = ['[[route.upstream]]'];
  lines.push(`addr = "${esc(upstream.addr)}"`);
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

const renderRoute = (route: RouteConfig): string => {
  const lines = ['[[route]]'];
  lines.push(`name = "${esc(route.name)}"`);
  if (route.host.trim()) {
    lines.push(`host = "${esc(route.host)}"`);
  }
  lines.push(`path_prefix = "${esc(route.path_prefix || '/')}"`);
  lines.push(`is_default = ${toTomlBool(route.is_default)}`);
  lines.push(`lb = "${route.lb}"`);
  lines.push(`max_retries = ${Math.max(0, route.max_retries)}`);
  lines.push(`retry_backoff_ms = ${Math.max(0, route.retry_backoff_ms)}`);
  lines.push('');
  lines.push('[route.circuit_breaker]');
  lines.push(`enabled = ${toTomlBool(route.circuit_breaker.enabled)}`);
  lines.push(
    `consecutive_failures = ${Math.max(1, route.circuit_breaker.consecutive_failures)}`
  );
  lines.push(`open_ms = ${Math.max(1, route.circuit_breaker.open_ms)}`);
  lines.push('');

  const upstreams = route.upstreams.map(renderUpstream).join('\n\n');
  return `${lines.join('\n')}${upstreams}`;
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

  const routeBlocks = config.routes.map(renderRoute);
  if (routeBlocks.length > 0) {
    lines.push('');
    lines.push(routeBlocks.join('\n\n'));
  }

  return `${lines.join('\n')}\n`;
};
