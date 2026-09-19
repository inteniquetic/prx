import type { RouteConfig, ServiceConfig } from './types/config';

/**
 * Client-side validation for a service, mirroring the service half of
 * `PrxConfig::validate` and the checks in the admin handlers.
 *
 * Same rules as `routeValidation.ts`: only repeat what the server also checks,
 * and let the server's answer win. It becomes a wrapper over the JSON Schema
 * once T205 exports one.
 */

export type FieldErrors = Record<string, string[]>;

export interface ServiceValidationContext {
  services: ServiceConfig[];
  /** The name the service had before editing, so it does not clash with itself. */
  editingName?: string | null;
}

const HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;

const add = (errors: FieldErrors, field: string, message: string) => {
  (errors[field] ??= []).push(message);
};

export function validateService(
  service: ServiceConfig,
  context: ServiceValidationContext
): FieldErrors {
  const errors: FieldErrors = {};

  if (!service.name.trim()) {
    add(errors, 'name', 'A service needs a name.');
  } else if (/\s/.test(service.name)) {
    add(errors, 'name', 'Service names cannot contain spaces.');
  } else if (
    context.services.some(
      (other) => other.name === service.name && other.name !== context.editingName
    )
  ) {
    add(errors, 'name', `A service called “${service.name}” already exists.`);
  }

  if (service.upstreams.length === 0) {
    add(errors, 'upstreams', 'A service needs at least one upstream.');
  }

  service.upstreams.forEach((upstream, index) => {
    if (!upstream.addr.trim()) {
      add(errors, `upstreams.${index}.addr`, 'An upstream needs an address.');
    } else if (!/^\[?[^\s]+\]?:\d+$/.test(upstream.addr.trim())) {
      add(errors, `upstreams.${index}.addr`, 'Use host:port, for example 10.0.0.5:8080.');
    }
    if (upstream.weight < 1 || upstream.weight > 256) {
      add(errors, `upstreams.${index}.weight`, 'Weight runs from 1 to 256.');
    }
  });

  if (service.upstreams.length > 0 && service.upstreams.every((upstream) => !upstream.enabled)) {
    // Not an error the server rejects, but a service in this state answers
    // nothing, so it is worth blocking a save that was probably a slip.
    add(errors, 'upstreams', 'Every upstream is drained, so this service has nowhere to send.');
  }

  if (
    !Number.isFinite(service.retry_budget_ratio) ||
    service.retry_budget_ratio < 0 ||
    service.retry_budget_ratio > 10
  ) {
    add(errors, 'retry_budget_ratio', 'Runs from 0 (off) to 10.');
  }
  if (service.retry_budget_window_ms <= 0) {
    add(errors, 'retry_budget_window_ms', 'The window has to be longer than zero.');
  }

  if (service.sticky.enabled) {
    if (!service.sticky.name.trim()) {
      add(errors, 'sticky.name', 'Affinity needs a cookie or header name.');
    } else if (service.sticky.mode === 'header' && !HEADER_NAME.test(service.sticky.name)) {
      add(errors, 'sticky.name', `“${service.sticky.name}” is not a valid header name.`);
    }
  }

  if (service.health_check.enabled) {
    if (service.health_check.interval_ms <= 0) {
      add(errors, 'health_check.interval_ms', 'Probes need an interval longer than zero.');
    }
    if (service.health_check.timeout_ms <= 0) {
      add(errors, 'health_check.timeout_ms', 'A probe with no timeout never finishes.');
    }
    if (service.health_check.healthy_threshold <= 0) {
      add(errors, 'health_check.healthy_threshold', 'Has to be at least one probe.');
    }
    if (service.health_check.unhealthy_threshold <= 0) {
      add(errors, 'health_check.unhealthy_threshold', 'Has to be at least one probe.');
    }
    if (service.health_check.kind === 'http') {
      if (!service.health_check.path.startsWith('/')) {
        add(errors, 'health_check.path', 'The probe path has to start with “/”.');
      }
      if (service.health_check.expected_status.length === 0) {
        add(errors, 'health_check.expected_status', 'Pick at least one status that counts as healthy.');
      }
    }
  }

  if (service.circuit_breaker.enabled) {
    if (service.circuit_breaker.consecutive_failures <= 0) {
      add(errors, 'circuit_breaker.consecutive_failures', 'Has to be at least one failure.');
    }
    if (service.circuit_breaker.open_ms <= 0) {
      add(errors, 'circuit_breaker.open_ms', 'The breaker has to stay open for some time.');
    }
  }

  return errors;
}

export const hasErrors = (errors: FieldErrors): boolean =>
  Object.values(errors).some((messages) => messages.length > 0);

export function serviceWarnings(service: ServiceConfig, routes: RouteConfig[]): string[] {
  const warnings: string[] = [];
  const using = routes.filter((route) => route.service === service.name);

  if (using.length === 0) {
    warnings.push('No route points at this service yet, so nothing reaches it.');
  }

  const drained = service.upstreams.filter((upstream) => !upstream.enabled).length;
  if (drained > 0 && drained < service.upstreams.length) {
    warnings.push(
      `${drained} upstream${drained === 1 ? ' is' : 's are'} drained and taking no traffic.`
    );
  }

  if (service.sticky.enabled && service.sticky.mode !== 'cookie') {
    warnings.push(
      'Hash-based affinity reshuffles some clients whenever an upstream is added or removed. Cookie mode pins exactly.'
    );
  }

  if (service.health_check.enabled && service.health_check.kind === 'tcp') {
    warnings.push(
      'A TCP probe only proves the port is open. An HTTP probe proves the app behind it answers.'
    );
  }

  return warnings;
}

/** Routes that would break if this service were deleted. */
export const routesUsing = (serviceName: string, routes: RouteConfig[]): RouteConfig[] =>
  routes.filter((route) => route.service === serviceName);

/**
 * Turns a rejection from the admin API into a message at the field it is about.
 * Same approach and the same caveat as the route form: there is no error-code
 * contract until T203, so this reads the sentence.
 */
export function mapServerError(message: string): { field: string | null; message: string } {
  const text = message.trim();
  const lower = text.toLowerCase();

  const FIELD_PATTERNS: [RegExp, string][] = [
    [/health_check\.interval_ms/, 'health_check.interval_ms'],
    [/health_check\.timeout_ms/, 'health_check.timeout_ms'],
    [/health_check\.healthy_threshold/, 'health_check.healthy_threshold'],
    [/health_check\.unhealthy_threshold/, 'health_check.unhealthy_threshold'],
    [/health_check\.path/, 'health_check.path'],
    [/health_check\.expected_status/, 'health_check.expected_status'],
    [/circuit_breaker\.consecutive_failures|consecutive_failures/, 'circuit_breaker.consecutive_failures'],
    [/circuit_breaker\.open_ms|open_ms/, 'circuit_breaker.open_ms'],
    [/retry_budget_ratio/, 'retry_budget_ratio'],
    [/retry_budget_window_ms/, 'retry_budget_window_ms'],
    [/sticky\.name/, 'sticky.name'],
    [/upstream|at least one/, 'upstreams'],
    [/already exists|duplicate service name|service_name_cannot_be_empty|name in body/, 'name']
  ];

  for (const [pattern, field] of FIELD_PATTERNS) {
    if (pattern.test(lower)) return { field, message: text };
  }

  return { field: null, message: text };
}
