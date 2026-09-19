import { HTTP_METHODS, type RouteConfig } from './types/config';

/**
 * Client-side validation for a route, mirroring `PrxConfig::validate` and the
 * checks in the admin handlers.
 *
 * Two rules keep this from becoming a second source of truth: it only repeats
 * checks the server also makes, and the server's answer always wins — a save
 * that comes back rejected is shown at the field it names. Once T205 exports a
 * JSON Schema this file becomes a thin wrapper over it; until then these are
 * written out, and a mismatch shows up as a save that passed here and failed
 * there rather than as a route the proxy will not load.
 */

export type FieldErrors = Record<string, string[]>;

export interface RouteValidationContext {
  routes: RouteConfig[];
  serviceNames: string[];
  /** The name the route had before editing, so it does not clash with itself. */
  editingName?: string | null;
}

const HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const RATE_LIMIT_KEY = /^(client_ip|route|header:[!#$%&'*+\-.^_`|~0-9A-Za-z]+)$/;

const add = (errors: FieldErrors, field: string, message: string) => {
  (errors[field] ??= []).push(message);
};

function validateHeaderRules(
  errors: FieldErrors,
  rules: RouteConfig['request_headers'],
  field: string
) {
  for (const [name, value] of [...Object.entries(rules.set), ...Object.entries(rules.add)]) {
    if (!name.trim()) {
      add(errors, field, 'A header rule with no name cannot be applied.');
      continue;
    }
    if (!HEADER_NAME.test(name)) {
      add(errors, field, `“${name}” is not a valid header name.`);
    }
    if (value.includes('\n') || value.includes('\r')) {
      add(errors, field, `The value for “${name}” cannot contain a line break.`);
    }
  }
  for (const name of rules.remove) {
    if (!HEADER_NAME.test(name)) {
      add(errors, field, `“${name}” is not a valid header name.`);
    }
  }
}

export function validateRoute(
  route: RouteConfig,
  context: RouteValidationContext
): FieldErrors {
  const errors: FieldErrors = {};

  // --- identity -----------------------------------------------------------
  if (!route.name.trim()) {
    add(errors, 'name', 'A route needs a name.');
  } else if (/\s/.test(route.name)) {
    add(errors, 'name', 'Route names cannot contain spaces.');
  } else if (
    context.routes.some(
      (other) => other.name === route.name && other.name !== context.editingName
    )
  ) {
    add(errors, 'name', `A route called “${route.name}” already exists.`);
  }

  if (!route.service.trim()) {
    add(errors, 'service', 'Pick the service this route sends traffic to.');
  } else if (!context.serviceNames.includes(route.service)) {
    add(errors, 'service', `There is no service called “${route.service}”.`);
  }

  // --- matching -----------------------------------------------------------
  const path = route.path_prefix;
  if (!path.trim()) {
    add(errors, 'path_prefix', 'A path prefix is required — use “/” to match everything.');
  } else if (!path.startsWith('/')) {
    add(errors, 'path_prefix', 'The path prefix has to start with “/”.');
  } else if (/\s/.test(path)) {
    add(errors, 'path_prefix', 'The path prefix cannot contain spaces.');
  }

  const host = route.host.trim();
  if (host) {
    if (/^https?:\/\//i.test(host)) {
      add(errors, 'host', 'Enter a host name on its own, without the scheme.');
    } else if (/:\d+$/.test(host)) {
      add(errors, 'host', 'prx strips the port before matching, so drop it here.');
    } else if (/\s/.test(host)) {
      add(errors, 'host', 'A host cannot contain spaces.');
    } else if (host.includes('*') && !host.startsWith('*.')) {
      add(errors, 'host', 'The only wildcard form is “*.example.com”.');
    } else if (host === '*.') {
      add(errors, 'host', 'A wildcard needs a domain after it, like “*.example.com”.');
    }
  }

  for (const method of route.methods) {
    if (!HTTP_METHODS.includes(method.toUpperCase() as (typeof HTTP_METHODS)[number])) {
      add(errors, 'methods', `prx does not know the method “${method}”.`);
    }
  }

  // --- limits -------------------------------------------------------------
  if (route.rate_limit.enabled) {
    if (!RATE_LIMIT_KEY.test(route.rate_limit.key)) {
      add(
        errors,
        'rate_limit.key',
        'Use “client_ip”, “route”, or “header:<Name>”.'
      );
    }
    if (route.rate_limit.requests_per_second <= 0) {
      add(errors, 'rate_limit.requests_per_second', 'Has to be at least 1 request per second.');
    }
    if (route.rate_limit.response_status < 400 || route.rate_limit.response_status > 599) {
      add(errors, 'rate_limit.response_status', 'Has to be a 4xx or 5xx status.');
    }
    if (route.rate_limit.max_entries <= 0) {
      add(errors, 'rate_limit.max_entries', 'Has to track at least one key.');
    }
  }

  if (
    route.concurrency_limit.response_status < 400 ||
    route.concurrency_limit.response_status > 599
  ) {
    add(errors, 'concurrency_limit.response_status', 'Has to be a 4xx or 5xx status.');
  }

  // --- cache --------------------------------------------------------------
  if (route.cache.enabled) {
    if (route.cache.ttl_ms <= 0) {
      add(errors, 'cache.ttl_ms', 'A cache with no lifetime stores nothing.');
    }
    if (route.cache.max_body_bytes <= 0) {
      add(errors, 'cache.max_body_bytes', 'Has to be at least one byte.');
    }
    if (route.cache.cache_status_codes.length === 0) {
      add(errors, 'cache.cache_status_codes', 'Pick at least one status code to store.');
    }
    for (const name of route.cache.vary_headers) {
      if (!HEADER_NAME.test(name)) {
        add(errors, 'cache.vary_headers', `“${name}” is not a valid header name.`);
      }
    }
  }

  validateHeaderRules(errors, route.request_headers, 'request_headers');
  validateHeaderRules(errors, route.response_headers, 'response_headers');

  return errors;
}

/** True when nothing in the form blocks a save. */
export const hasErrors = (errors: FieldErrors): boolean =>
  Object.values(errors).some((messages) => messages.length > 0);

/**
 * Warnings are things worth saying out loud that are not reasons to refuse the
 * save.
 */
export function routeWarnings(route: RouteConfig, context: RouteValidationContext): string[] {
  const warnings: string[] = [];

  const currentDefault = context.routes.find(
    (other) => other.is_default && other.name !== context.editingName
  );
  if (route.is_default && currentDefault) {
    warnings.push(
      `“${currentDefault.name}” is the fallback today. Saving this moves the fallback here, and the config can only have one.`
    );
  }

  if (!route.enabled) {
    warnings.push('This route is disabled, so it will not take any traffic until it is enabled.');
  }

  if (route.cache.enabled && route.methods.some((method) => method.toUpperCase() !== 'GET')) {
    warnings.push('prx only caches GET responses; the other methods pass straight through.');
  }

  return warnings;
}

/**
 * Turns a rejection from the admin API into a message at the field it is about.
 *
 * The server speaks in sentences like `route 'api' cache.ttl_ms must be > 0`
 * (there is no error-code contract until T203), so the field is recovered by
 * looking for the names it mentions. Anything unrecognised stays a form-level
 * message rather than being hidden.
 */
export function mapServerError(message: string): { field: string | null; message: string } {
  const text = message.trim();
  const lower = text.toLowerCase();

  const FIELD_PATTERNS: [RegExp, string][] = [
    [/cache\.ttl_ms/, 'cache.ttl_ms'],
    [/cache\.max_body_bytes/, 'cache.max_body_bytes'],
    [/cache\.cache_status_codes/, 'cache.cache_status_codes'],
    [/cache\.vary_headers/, 'cache.vary_headers'],
    [/rate_limit\.key/, 'rate_limit.key'],
    [/rate_limit\.requests_per_second/, 'rate_limit.requests_per_second'],
    [/rate_limit\.response_status/, 'rate_limit.response_status'],
    [/rate_limit\.max_entries/, 'rate_limit.max_entries'],
    [/concurrency_limit\.response_status/, 'concurrency_limit.response_status'],
    [/path_prefix/, 'path_prefix'],
    [/unsupported http method|method/, 'methods'],
    [/request_headers/, 'request_headers'],
    [/response_headers/, 'response_headers'],
    [/is_default|only one route can be marked/, 'is_default'],
    [/service '[^']*' not found|unknown service|service_cannot_be_empty/, 'service'],
    [/already exists|route_name_cannot_be_empty/, 'name'],
    [/host/, 'host']
  ];

  for (const [pattern, field] of FIELD_PATTERNS) {
    if (pattern.test(lower)) return { field, message: text };
  }

  return { field: null, message: text };
}
