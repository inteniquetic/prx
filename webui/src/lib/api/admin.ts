import { normalizePrxConfig } from '../configNormalize';
import type { PrxConfig } from '../types/config';

const ADMIN_CONFIG_ENDPOINT = '/web/config';
const ADMIN_ROUTE_HEALTH_ENDPOINT = '/web/health/routes';
const REQUEST_TIMEOUT_MS = 10000;

const fetchWithTimeout = async (
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS
): Promise<Response> => {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs);

  try {
    return await fetch(input, {
      ...init,
      signal: controller.signal
    });
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
