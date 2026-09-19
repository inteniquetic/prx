/**
 * What a config change means, in words (T307).
 *
 * A unified diff says which lines moved. It does not say that the weight of an
 * upstream went from 1 to 5, or that the route everything falls back to is now
 * a different one — and that is the part worth reading twice before pressing
 * Apply. Both sides come from the admin API's own JSON, so what is compared
 * here is the config the proxy would load, not the text someone typed.
 */

import type {
  HeaderRules,
  PrxConfig,
  RouteConfig,
  ServiceConfig,
  UpstreamConfig
} from './types/config';

export type ChangeKind = 'add' | 'remove' | 'modify';

export type ChangeScope = 'server' | 'observability' | 'tls' | 'service' | 'route';

export interface ConfigChange {
  kind: ChangeKind;
  scope: ChangeScope;
  /** What changed, e.g. `route "api-v1"`. */
  subject: string;
  /** The detail lines under it, e.g. `weight 1 → 5`. */
  details: string[];
  /** True for changes that alter where traffic goes. */
  traffic: boolean;
}

export interface ChangeSummary {
  changes: ConfigChange[];
  /** One line for a button or a toast: "1 route added, 1 service removed". */
  headline: string;
  /** True when both configs describe the same proxy. */
  identical: boolean;
}

const quote = (value: string): string => `“${value}”`;

const show = (value: unknown): string => {
  if (value === null || value === undefined || value === '') return 'unset';
  if (typeof value === 'boolean') return value ? 'on' : 'off';
  if (Array.isArray(value)) return value.length === 0 ? 'none' : value.join(', ');
  return String(value);
};

/** `field: old → new`, or nothing when the two are the same. */
function fieldChange(label: string, before: unknown, after: unknown): string | null {
  const sameArray =
    Array.isArray(before) && Array.isArray(after) && before.join('\u0000') === after.join('\u0000');
  if (sameArray || before === after) return null;
  return `${label} ${show(before)} → ${show(after)}`;
}

function collect(
  details: string[],
  label: string,
  before: unknown,
  after: unknown
): void {
  const change = fieldChange(label, before, after);
  if (change) details.push(change);
}

function headerSummary(rules: HeaderRules): string {
  const parts: string[] = [];
  const set = Object.keys(rules.set ?? {});
  const add = Object.keys(rules.add ?? {});
  if (set.length) parts.push(`set ${set.join(', ')}`);
  if (add.length) parts.push(`add ${add.join(', ')}`);
  if (rules.remove?.length) parts.push(`remove ${rules.remove.join(', ')}`);
  return parts.length ? parts.join('; ') : 'none';
}

function upstreamLabel(upstream: UpstreamConfig): string {
  return upstream.addr;
}

function compareUpstreams(
  before: UpstreamConfig[],
  after: UpstreamConfig[],
  details: string[]
): boolean {
  let traffic = false;
  const beforeByAddr = new Map(before.map((upstream) => [upstream.addr, upstream]));
  const afterByAddr = new Map(after.map((upstream) => [upstream.addr, upstream]));

  for (const upstream of after) {
    if (!beforeByAddr.has(upstream.addr)) {
      details.push(`upstream ${upstreamLabel(upstream)} added`);
      traffic = true;
    }
  }
  for (const upstream of before) {
    if (!afterByAddr.has(upstream.addr)) {
      details.push(`upstream ${upstreamLabel(upstream)} removed`);
      traffic = true;
    }
  }

  for (const [addr, next] of afterByAddr) {
    const previous = beforeByAddr.get(addr);
    if (!previous) continue;

    const fields: string[] = [];
    collect(fields, 'weight', previous.weight, next.weight);
    collect(fields, 'enabled', previous.enabled, next.enabled);
    collect(fields, 'tls', previous.tls, next.tls);
    collect(fields, 'sni', previous.sni, next.sni);
    collect(fields, 'verify_cert', previous.verify_cert, next.verify_cert);
    collect(fields, 'verify_hostname', previous.verify_hostname, next.verify_hostname);
    collect(fields, 'connect_timeout_ms', previous.connect_timeout_ms, next.connect_timeout_ms);
    collect(fields, 'read_timeout_ms', previous.read_timeout_ms, next.read_timeout_ms);
    collect(fields, 'write_timeout_ms', previous.write_timeout_ms, next.write_timeout_ms);
    collect(fields, 'idle_timeout_ms', previous.idle_timeout_ms, next.idle_timeout_ms);

    if (fields.length > 0) {
      details.push(`upstream ${addr}: ${fields.join(', ')}`);
      if (previous.weight !== next.weight || previous.enabled !== next.enabled) traffic = true;
    }
  }

  return traffic;
}

function compareService(before: ServiceConfig, after: ServiceConfig): ConfigChange | null {
  const details: string[] = [];
  let traffic = false;

  collect(details, 'load balancing', before.lb, after.lb);
  if (before.lb !== after.lb) traffic = true;
  collect(details, 'max_retries', before.max_retries, after.max_retries);
  collect(details, 'retry_backoff_ms', before.retry_backoff_ms, after.retry_backoff_ms);
  collect(details, 'retry_budget_ratio', before.retry_budget_ratio, after.retry_budget_ratio);
  collect(details, 'request_timeout_ms', before.request_timeout_ms, after.request_timeout_ms);
  collect(details, 'upstream HTTP/2', before.upstream_h2, after.upstream_h2);
  collect(
    details,
    'circuit breaker',
    before.circuit_breaker.enabled,
    after.circuit_breaker.enabled
  );
  collect(details, 'health check', before.health_check.enabled, after.health_check.enabled);
  collect(details, 'sticky sessions', before.sticky.enabled, after.sticky.enabled);
  if (before.sticky.enabled && after.sticky.enabled) {
    collect(details, 'sticky mode', before.sticky.mode, after.sticky.mode);
  }

  if (compareUpstreams(before.upstreams, after.upstreams, details)) traffic = true;

  if (details.length === 0) return null;
  return {
    kind: 'modify',
    scope: 'service',
    subject: `service ${quote(after.name)}`,
    details,
    traffic
  };
}

function compareRoute(before: RouteConfig, after: RouteConfig): ConfigChange | null {
  const details: string[] = [];
  let traffic = false;

  collect(details, 'service', before.service, after.service);
  collect(details, 'host', before.host, after.host);
  collect(details, 'path_prefix', before.path_prefix, after.path_prefix);
  collect(details, 'methods', before.methods, after.methods);
  collect(details, 'default route', before.is_default, after.is_default);
  collect(details, 'enabled', before.enabled, after.enabled);
  if (
    before.service !== after.service ||
    before.host !== after.host ||
    before.path_prefix !== after.path_prefix ||
    before.is_default !== after.is_default ||
    before.enabled !== after.enabled ||
    before.methods.join(',') !== after.methods.join(',')
  ) {
    traffic = true;
  }

  collect(
    details,
    'request headers',
    headerSummary(before.request_headers),
    headerSummary(after.request_headers)
  );
  collect(
    details,
    'response headers',
    headerSummary(before.response_headers),
    headerSummary(after.response_headers)
  );
  collect(details, 'rate limit', before.rate_limit.enabled, after.rate_limit.enabled);
  if (before.rate_limit.enabled && after.rate_limit.enabled) {
    collect(
      details,
      'rate limit req/s',
      before.rate_limit.requests_per_second,
      after.rate_limit.requests_per_second
    );
    collect(details, 'rate limit key', before.rate_limit.key, after.rate_limit.key);
  }
  collect(
    details,
    'concurrency limit',
    before.concurrency_limit.max_concurrent,
    after.concurrency_limit.max_concurrent
  );
  collect(details, 'cache', before.cache.enabled, after.cache.enabled);
  if (before.cache.enabled && after.cache.enabled) {
    collect(details, 'cache ttl_ms', before.cache.ttl_ms, after.cache.ttl_ms);
  }

  if (details.length === 0) return null;
  return {
    kind: 'modify',
    scope: 'route',
    subject: `route ${quote(after.name)}`,
    details,
    traffic
  };
}

function compareServer(before: PrxConfig, after: PrxConfig, changes: ConfigChange[]): void {
  const details: string[] = [];
  collect(details, 'listen', before.server.listen, after.server.listen);
  collect(details, 'health_path', before.server.health_path, after.server.health_path);
  collect(details, 'ready_path', before.server.ready_path, after.server.ready_path);
  collect(details, 'threads', before.server.threads, after.server.threads);
  collect(
    details,
    'config_reload_debounce_ms',
    before.server.config_reload_debounce_ms,
    after.server.config_reload_debounce_ms
  );
  if (details.length > 0) {
    changes.push({
      kind: 'modify',
      scope: 'server',
      subject: 'server',
      details,
      traffic: before.server.listen.join(',') !== after.server.listen.join(',')
    });
  }

  const tlsDetails: string[] = [];
  if (!before.server.tls && after.server.tls) {
    tlsDetails.push(`TLS listener ${after.server.tls.listen} added`);
  } else if (before.server.tls && !after.server.tls) {
    tlsDetails.push(`TLS listener ${before.server.tls.listen} removed`);
  } else if (before.server.tls && after.server.tls) {
    collect(tlsDetails, 'listen', before.server.tls.listen, after.server.tls.listen);
    collect(tlsDetails, 'cert_path', before.server.tls.cert_path, after.server.tls.cert_path);
    collect(tlsDetails, 'key_path', before.server.tls.key_path, after.server.tls.key_path);
    collect(tlsDetails, 'HTTP/2', before.server.tls.enable_h2, after.server.tls.enable_h2);
    collect(
      tlsDetails,
      'ACME',
      before.server.tls.acme?.enabled ?? false,
      after.server.tls.acme?.enabled ?? false
    );
    collect(
      tlsDetails,
      'ACME domains',
      before.server.tls.acme?.domains ?? [],
      after.server.tls.acme?.domains ?? []
    );
  }
  if (tlsDetails.length > 0) {
    changes.push({ kind: 'modify', scope: 'tls', subject: 'TLS', details: tlsDetails, traffic: true });
  }

  const observability: string[] = [];
  collect(observability, 'log level', before.observability.log_level, after.observability.log_level);
  collect(observability, 'access log', before.observability.access_log, after.observability.access_log);
  collect(
    observability,
    'prometheus_listen',
    before.observability.prometheus_listen,
    after.observability.prometheus_listen
  );
  if (observability.length > 0) {
    changes.push({
      kind: 'modify',
      scope: 'observability',
      subject: 'observability',
      details: observability,
      traffic: false
    });
  }
}

/** Everything that differs between two configs, as a list a human can read. */
export function summarizeChanges(before: PrxConfig, after: PrxConfig): ChangeSummary {
  const changes: ConfigChange[] = [];

  compareServer(before, after, changes);

  const beforeServices = new Map(before.services.map((service) => [service.name, service]));
  const afterServices = new Map(after.services.map((service) => [service.name, service]));
  for (const service of after.services) {
    if (!beforeServices.has(service.name)) {
      changes.push({
        kind: 'add',
        scope: 'service',
        subject: `service ${quote(service.name)}`,
        details: [
          `${service.upstreams.length} upstream${service.upstreams.length === 1 ? '' : 's'}: ${service.upstreams
            .map(upstreamLabel)
            .join(', ')}`
        ],
        traffic: true
      });
    }
  }
  for (const service of before.services) {
    if (!afterServices.has(service.name)) {
      changes.push({
        kind: 'remove',
        scope: 'service',
        subject: `service ${quote(service.name)}`,
        details: [],
        traffic: true
      });
    }
  }
  for (const [name, next] of afterServices) {
    const previous = beforeServices.get(name);
    if (!previous) continue;
    const change = compareService(previous, next);
    if (change) changes.push(change);
  }

  const beforeRoutes = new Map(before.routes.map((route) => [route.name, route]));
  const afterRoutes = new Map(after.routes.map((route) => [route.name, route]));
  for (const route of after.routes) {
    if (!beforeRoutes.has(route.name)) {
      changes.push({
        kind: 'add',
        scope: 'route',
        subject: `route ${quote(route.name)}`,
        details: [
          `${route.host || 'any host'} ${route.path_prefix} → service ${quote(route.service)}`
        ],
        traffic: true
      });
    }
  }
  for (const route of before.routes) {
    if (!afterRoutes.has(route.name)) {
      changes.push({
        kind: 'remove',
        scope: 'route',
        subject: `route ${quote(route.name)}`,
        details: [`${route.host || 'any host'} ${route.path_prefix}`],
        traffic: true
      });
    }
  }
  for (const [name, next] of afterRoutes) {
    const previous = beforeRoutes.get(name);
    if (!previous) continue;
    const change = compareRoute(previous, next);
    if (change) changes.push(change);
  }

  // Route order decides ties, so a reordered file is a real change even when
  // every route in it is the same.
  const beforeOrder = before.routes.map((route) => route.name).join('\u0000');
  const afterOrder = after.routes.map((route) => route.name).join('\u0000');
  if (
    beforeOrder !== afterOrder &&
    before.routes.length === after.routes.length &&
    before.routes.every((route) => afterRoutes.has(route.name))
  ) {
    changes.push({
      kind: 'modify',
      scope: 'route',
      subject: 'route order',
      details: ['a tie on host and path is settled by file order'],
      traffic: true
    });
  }

  return { changes, headline: headline(changes), identical: changes.length === 0 };
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`;
}

/** "1 route added, 2 services changed" — the line that goes on the button. */
function headline(changes: ConfigChange[]): string {
  if (changes.length === 0) return 'No changes';

  const counts = new Map<string, number>();
  for (const change of changes) {
    const noun =
      change.scope === 'service' ? 'service' : change.scope === 'route' ? 'route' : change.subject;
    const verb = change.kind === 'add' ? 'added' : change.kind === 'remove' ? 'removed' : 'changed';
    const key = `${noun}\u0000${verb}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }

  const parts: string[] = [];
  for (const [key, count] of counts) {
    const [noun, verb] = key.split('\u0000');
    parts.push(
      noun === 'service' || noun === 'route'
        ? `${plural(count, noun)} ${verb}`
        : `${noun} ${verb}`
    );
  }
  return parts.join(', ');
}
