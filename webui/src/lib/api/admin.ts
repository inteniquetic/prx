import { normalizePrxConfig } from '../configNormalize';
import { reportFailure, reportSuccess } from '../stores/connection';
import type { PrxConfig, ServiceConfig, RouteConfig } from '../types/config';

const ADMIN_CONFIG_ENDPOINT = '/web/config';
const ADMIN_ROUTE_HEALTH_ENDPOINT = '/web/health/routes';
const ADMIN_ROUTE_TEST_ENDPOINT = '/web/routes/test';
const ADMIN_SERVICE_STATUS_ENDPOINT = '/web/services/status';
const ADMIN_UPSTREAM_TEST_ENDPOINT = '/web/upstreams/test';
const ADMIN_SERVICES_ENDPOINT = '/admin/services';
const ADMIN_ROUTES_ENDPOINT = '/admin/routes';
const REQUEST_TIMEOUT_MS = 10000;

const fetchWithTimeout = async (
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS
): Promise<Response> => {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(input, {
      ...init,
      signal: controller.signal
    });
    // Reachability, not correctness: a 500 still proves the admin API is there,
    // and the caller is the one that decides what the status code means.
    reportSuccess();
    return response;
  } catch (error) {
    reportFailure(error);
    throw error;
  } finally {
    window.clearTimeout(timeoutId);
  }
};

const buildHttpError = async (operation: string, response: Response): Promise<Error> => {
  const bodyText = (await response.text()).trim();
  const reason = bodyText || response.statusText || 'unknown_error';
  return new Error(`${operation} failed (${response.status}): ${reason}`);
};

export const loadConfigFromAdmin = async (): Promise<PrxConfig> => {
  const response = await fetchWithTimeout(`${ADMIN_CONFIG_ENDPOINT}?format=json`, {
    method: 'GET',
    headers: {
      Accept: 'application/json'
    },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('load_config', response);
  }

  const payload = (await response.json()) as Partial<PrxConfig>;
  return normalizePrxConfig(payload);
};

export const saveTomlToAdmin = async (tomlText: string): Promise<string> => {
  const response = await fetchWithTimeout(ADMIN_CONFIG_ENDPOINT, {
    method: 'PUT',
    headers: {
      'Content-Type': 'text/plain; charset=utf-8'
    },
    body: tomlText
  });

  const bodyText = (await response.text()).trim();
  if (!response.ok) {
    const reason = bodyText || response.statusText || 'unknown_error';
    throw new Error(`save_config failed (${response.status}): ${reason}`);
  }

  return bodyText || 'config_applied';
};

export interface RouteHealthUpstream {
  addr: string;
  timeout_ms: number;
  healthy: boolean;
  latency_ms: number | null;
  error: string | null;
}

export interface RouteHealthItem {
  route_index: number;
  name: string;
  service: string;
  host: string;
  path_prefix: string;
  healthy: boolean;
  reachable_upstreams: number;
  total_upstreams: number;
  upstreams: RouteHealthUpstream[];
}

export interface RouteHealthResponse {
  checked_at_epoch_ms: number;
  timeout_ms: number;
  routes: RouteHealthItem[];
}

export const loadRouteHealthFromAdmin = async (
  timeoutMs = 1200,
  tomlText?: string
): Promise<RouteHealthResponse> => {
  const query = new URLSearchParams({
    timeout_ms: String(Math.max(100, Math.min(10000, Math.floor(timeoutMs))))
  });
  const method = tomlText && tomlText.trim().length > 0 ? 'POST' : 'GET';
  const response = await fetchWithTimeout(
    `${ADMIN_ROUTE_HEALTH_ENDPOINT}?${query.toString()}`,
    {
      method,
      headers: {
        Accept: 'application/json',
        ...(method === 'POST' ? { 'Content-Type': 'text/plain; charset=utf-8' } : {})
      },
      cache: 'no-store',
      ...(method === 'POST' ? { body: tomlText } : {})
    },
    REQUEST_TIMEOUT_MS + timeoutMs
  );

  if (!response.ok) {
    throw await buildHttpError('load_route_health', response);
  }

  return (await response.json()) as RouteHealthResponse;
};

// Service CRUD API functions

export const createService = async (service: ServiceConfig): Promise<ServiceConfig> => {
  const response = await fetchWithTimeout(ADMIN_SERVICES_ENDPOINT, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json'
    },
    body: JSON.stringify(service)
  });

  const bodyText = (await response.text()).trim();
  if (!response.ok) {
    const reason = bodyText || response.statusText || 'unknown_error';
    throw new Error(`create_service failed (${response.status}): ${reason}`);
  }

  return service;
};

export const updateService = async (name: string, service: ServiceConfig): Promise<ServiceConfig> => {
  const response = await fetchWithTimeout(`${ADMIN_SERVICES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json'
    },
    body: JSON.stringify(service)
  });

  const bodyText = (await response.text()).trim();
  if (!response.ok) {
    const reason = bodyText || response.statusText || 'unknown_error';
    throw new Error(`update_service failed (${response.status}): ${reason}`);
  }

  return service;
};

export const deleteService = async (name: string): Promise<void> => {
  const response = await fetchWithTimeout(`${ADMIN_SERVICES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'DELETE'
  });

  if (!response.ok) {
    throw await buildHttpError('delete_service', response);
  }
};

export const listServices = async (): Promise<ServiceConfig[]> => {
  const response = await fetchWithTimeout(ADMIN_SERVICES_ENDPOINT, {
    method: 'GET',
    headers: {
      Accept: 'application/json'
    },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('list_services', response);
  }

  return (await response.json()) as ServiceConfig[];
};

export const getService = async (name: string): Promise<ServiceConfig> => {
  const response = await fetchWithTimeout(`${ADMIN_SERVICES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'GET',
    headers: {
      Accept: 'application/json'
    },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('get_service', response);
  }

  return (await response.json()) as ServiceConfig;
};

// Route CRUD API functions

export const createRoute = async (route: RouteConfig): Promise<RouteConfig> => {
  const response = await fetchWithTimeout(ADMIN_ROUTES_ENDPOINT, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json'
    },
    body: JSON.stringify(route)
  });

  const bodyText = (await response.text()).trim();
  if (!response.ok) {
    const reason = bodyText || response.statusText || 'unknown_error';
    throw new Error(`create_route failed (${response.status}): ${reason}`);
  }

  return route;
};

export const updateRoute = async (name: string, route: RouteConfig): Promise<RouteConfig> => {
  const response = await fetchWithTimeout(`${ADMIN_ROUTES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json'
    },
    body: JSON.stringify(route)
  });

  const bodyText = (await response.text()).trim();
  if (!response.ok) {
    const reason = bodyText || response.statusText || 'unknown_error';
    throw new Error(`update_route failed (${response.status}): ${reason}`);
  }

  return route;
};

export const deleteRoute = async (name: string): Promise<void> => {
  const response = await fetchWithTimeout(`${ADMIN_ROUTES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'DELETE'
  });

  if (!response.ok) {
    throw await buildHttpError('delete_route', response);
  }
};

export const listRoutes = async (): Promise<RouteConfig[]> => {
  const response = await fetchWithTimeout(ADMIN_ROUTES_ENDPOINT, {
    method: 'GET',
    headers: {
      Accept: 'application/json'
    },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('list_routes', response);
  }

  return (await response.json()) as RouteConfig[];
};

export const getRoute = async (name: string): Promise<RouteConfig> => {
  const response = await fetchWithTimeout(`${ADMIN_ROUTES_ENDPOINT}/${encodeURIComponent(name)}`, {
    method: 'GET',
    headers: {
      Accept: 'application/json'
    },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('get_route', response);
  }

  return (await response.json()) as RouteConfig;
};

// Route tester (T304). The server runs the request through the same matcher
// the proxy uses, so the answer cannot drift from live traffic.

export interface RouteTestRequest {
  method: string;
  host: string;
  path: string;
}

export interface RouteTestUpstream {
  addr: string;
  weight: number;
  available: boolean;
  circuit_open: boolean;
  probe_healthy: boolean;
  inflight: number;
  ewma_us: number;
}

export interface RouteTestResponse {
  outcome: 'matched' | 'method_not_allowed' | 'not_found';
  request: {
    method: string;
    host: string;
    normalized_host: string;
    path: string;
  };
  route: {
    index: number;
    name: string;
    host: string;
    path_prefix: string;
    methods: string[];
    is_default: boolean;
    enabled: boolean;
    matched_by: 'exact_host' | 'wildcard_host' | 'any_host' | 'default_route';
  } | null;
  service: {
    name: string;
    lb: string;
    selection: { deterministic: boolean; would_pick: string | null; note: string };
    upstreams: RouteTestUpstream[];
  } | null;
}

export const testRoute = async (request: RouteTestRequest): Promise<RouteTestResponse> => {
  const response = await fetchWithTimeout(ADMIN_ROUTE_TEST_ENDPOINT, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json'
    },
    body: JSON.stringify(request)
  });

  if (!response.ok) {
    throw await buildHttpError('test_route', response);
  }

  return (await response.json()) as RouteTestResponse;
};

// Live service state (T305). The config says what was asked for; this says what
// the running proxy is doing with it.

export interface UpstreamStatus {
  addr: string;
  enabled: boolean;
  weight: number;
  /** Share of the selection ring, read from the ring itself. */
  share: number;
  available: boolean;
  circuit_open: boolean;
  circuit_reopens_in_ms: number | null;
  consecutive_failures: number;
  probe_healthy: boolean;
  last_probe_ms_ago: number | null;
  inflight: number;
  ewma_us: number;
}

export interface ServiceStatus {
  name: string;
  lb: string;
  health_check_enabled: boolean;
  circuit_breaker_enabled: boolean;
  upstreams: UpstreamStatus[];
}

export interface ServiceStatusResponse {
  checked_at_epoch_ms: number;
  services: ServiceStatus[];
}

export const loadServiceStatus = async (): Promise<ServiceStatusResponse> => {
  const response = await fetchWithTimeout(ADMIN_SERVICE_STATUS_ENDPOINT, {
    method: 'GET',
    headers: { Accept: 'application/json' },
    cache: 'no-store'
  });

  if (!response.ok) {
    throw await buildHttpError('load_service_status', response);
  }

  return (await response.json()) as ServiceStatusResponse;
};

export interface UpstreamTestResult {
  addr: string;
  timeout_ms: number;
  healthy: boolean;
  latency_ms: number | null;
  error: string | null;
  source: string;
  last_probe_ms_ago: number | null;
}

export const testUpstream = async (
  addr: string,
  timeoutMs = 1200
): Promise<UpstreamTestResult> => {
  const response = await fetchWithTimeout(
    ADMIN_UPSTREAM_TEST_ENDPOINT,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ addr, timeout_ms: timeoutMs })
    },
    REQUEST_TIMEOUT_MS + timeoutMs
  );

  if (!response.ok) {
    throw await buildHttpError('test_upstream', response);
  }

  return (await response.json()) as UpstreamTestResult;
};
