/**
 * The live-stats API (T207).
 *
 * `GET /web/stats` is the first read — it carries five minutes of history so
 * the dashboard has a chart before the stream has said anything. After that the
 * SSE stream pushes one sample a second, and only the parts that changed.
 */

import { reportFailure, reportSuccess } from '../stores/connection';

const STATS_ENDPOINT = '/web/stats';
const STATS_STREAM_ENDPOINT = '/web/stats/stream';
const REQUEST_TIMEOUT_MS = 10000;

/** One second of traffic, as rates rather than counters. */
export interface StatsSample {
  epoch_ms: number;
  window_seconds: number;
  rps: number;
  rps_2xx: number;
  rps_3xx: number;
  rps_4xx: number;
  rps_5xx: number;
  error_ratio_4xx: number;
  error_ratio_5xx: number;
  /** `null` when no request finished in the window — not zero milliseconds. */
  p50_ms: number | null;
  p95_ms: number | null;
  p99_ms: number | null;
  inflight: number;
  upstreams_healthy: number;
  upstreams_total: number;
  /** `null` when no route has caching turned on. */
  cache_hit_ratio: number | null;
}

/** A route's share of the last minute. */
export interface RouteStats {
  name: string;
  requests: number;
  rps: number;
  error_ratio: number;
  requests_4xx: number;
  requests_5xx: number;
  p50_ms: number | null;
  p95_ms: number | null;
  p99_ms: number | null;
  window_seconds: number;
}

export type UpstreamLiveState = 'healthy' | 'degraded' | 'down' | 'drained';

export interface UpstreamHealth {
  addr: string;
  state: UpstreamLiveState;
  inflight: number;
  ewma_ms: number;
}

export interface ServiceHealth {
  name: string;
  healthy: number;
  total: number;
  upstreams: UpstreamHealth[];
}

export type StatsEventLevel = 'info' | 'warn' | 'error';

export interface StatsEvent {
  id: number;
  epoch_ms: number;
  level: StatsEventLevel;
  kind: string;
  message: string;
  target: string | null;
}

export interface StatsSnapshot {
  epoch_ms: number;
  interval_ms: number;
  seq: number;
  uptime_seconds: number;
  sample: StatsSample;
  history: StatsSample[];
  routes: RouteStats[];
  services: ServiceHealth[];
  /** Newest first. */
  events: StatsEvent[];
  stream_clients: number;
  max_stream_clients: number;
}

/**
 * One push. `routes` and `services` only ride along when they changed, so an
 * idle dashboard costs a couple of hundred bytes a second.
 */
export interface StatsTick {
  seq: number;
  sample: StatsSample;
  routes?: RouteStats[];
  services?: ServiceHealth[];
  events?: StatsEvent[];
}

export const loadStats = async (signal?: AbortSignal): Promise<StatsSnapshot> => {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  const onAbort = () => controller.abort();
  signal?.addEventListener('abort', onAbort);

  try {
    const response = await fetch(STATS_ENDPOINT, {
      method: 'GET',
      headers: { Accept: 'application/json' },
      cache: 'no-store',
      signal: controller.signal
    });
    reportSuccess();
    if (!response.ok) {
      const body = (await response.text()).trim();
      throw new Error(`load_stats failed (${response.status}): ${body || response.statusText}`);
    }
    return (await response.json()) as StatsSnapshot;
  } catch (error) {
    // An abort is this tab changing its mind, not the proxy going away.
    if (!(error instanceof DOMException && error.name === 'AbortError')) {
      reportFailure(error);
    }
    throw error;
  } finally {
    window.clearTimeout(timeoutId);
    signal?.removeEventListener('abort', onAbort);
  }
};

export const statsStreamUrl = (): string => STATS_STREAM_ENDPOINT;
