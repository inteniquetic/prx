import { writable } from 'svelte/store';

import {
  loadStats,
  statsStreamUrl,
  type RouteStats,
  type ServiceHealth,
  type StatsEvent,
  type StatsSample,
  type StatsSnapshot,
  type StatsTick
} from '../api/stats';

/**
 * The dashboard's live state.
 *
 * The page reads this and nothing else: one subscription feeds every tile,
 * chart and table, so a second of traffic is drawn once rather than fetched
 * five times. The stream is the source; polling is what happens when the stream
 * cannot be had, and the difference is visible in `status` rather than hidden
 * behind a chart that quietly stopped moving.
 */

/** Matches the server's ring buffer: five minutes at one sample a second. */
export const HISTORY_CAPACITY = 300;
const MAX_EVENTS = 50;
/** A stream that has said nothing for this long is not live any more. */
const STALE_AFTER_MS = 4000;
/**
 * How long a dropped stream is given to come back before the page starts
 * asking instead. Browsers reconnect an EventSource on their own, and a
 * reconnect that takes 200 ms is not worth telling anyone about.
 */
const FALLBACK_AFTER_MS = 1500;
const WATCHDOG_INTERVAL_MS = 1000;
const POLL_INTERVAL_MS = 2000;
/** How long to sit on polling before trying the stream again. */
const STREAM_RETRY_MS = 30000;

export type LiveStatus =
  | 'connecting'
  | 'live'
  /** The stream was refused or blocked; numbers still arrive, just by asking. */
  | 'polling'
  | 'reconnecting'
  | 'offline';

export interface LiveStatsState {
  status: LiveStatus;
  /** False until the first snapshot lands, so nothing reads zeros as data. */
  loaded: boolean;
  sample: StatsSample | null;
  /** Oldest first, which is the order a chart draws. */
  history: StatsSample[];
  routes: RouteStats[];
  services: ServiceHealth[];
  /** Newest first. */
  events: StatsEvent[];
  uptimeSeconds: number;
  intervalMs: number;
  lastUpdateMs: number | null;
  error: string | null;
}

const initialState: LiveStatsState = {
  status: 'connecting',
  loaded: false,
  sample: null,
  history: [],
  routes: [],
  services: [],
  events: [],
  uptimeSeconds: 0,
  intervalMs: 1000,
  lastUpdateMs: null,
  error: null
};

export const liveStats = writable<LiveStatsState>({ ...initialState });

const applySnapshot = (snapshot: StatsSnapshot) => {
  liveStats.update((state) => ({
    ...state,
    loaded: true,
    sample: snapshot.sample,
    history: snapshot.history.slice(-HISTORY_CAPACITY),
    routes: snapshot.routes,
    services: snapshot.services,
    events: snapshot.events.slice(0, MAX_EVENTS),
    uptimeSeconds: snapshot.uptime_seconds,
    intervalMs: snapshot.interval_ms || state.intervalMs,
    lastUpdateMs: Date.now(),
    error: null
  }));
};

const applyTick = (tick: StatsTick) => {
  liveStats.update((state) => {
    // A fixed-length window is what keeps a tab open for an hour from growing:
    // 300 samples in, one sample out.
    const history = state.history.concat(tick.sample);
    if (history.length > HISTORY_CAPACITY) {
      history.splice(0, history.length - HISTORY_CAPACITY);
    }

    const events = tick.events?.length
      ? tick.events
          .slice()
          .reverse()
          .concat(state.events)
          .slice(0, MAX_EVENTS)
      : state.events;

    return {
      ...state,
      loaded: true,
      status: 'live',
      sample: tick.sample,
      history,
      routes: tick.routes ?? state.routes,
      services: tick.services ?? state.services,
      events,
      lastUpdateMs: Date.now(),
      error: null
    };
  });
};

/**
 * An event-only push carries the sample the server last had, which is a second
 * old; taking it as a new sample would put a duplicate second on the chart.
 */
const applyEventOnly = (tick: StatsTick) => {
  if (!tick.events?.length) return;
  liveStats.update((state) => ({
    ...state,
    events: tick.events!.slice().reverse().concat(state.events).slice(0, MAX_EVENTS)
  }));
};

const setStatus = (status: LiveStatus, error?: string | null) => {
  liveStats.update((state) => ({
    ...state,
    status,
    error: error === undefined ? state.error : error
  }));
};

const toMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/**
 * Connects the dashboard to the live stats and returns a stop function.
 *
 * Everything it starts — the stream, the watchdog, the polling fallback and the
 * abort controller — is owned here and shut down by the returned function, so
 * leaving the page leaves nothing running. Calling it twice is a mistake the
 * caller would notice as double-speed charts, so the page starts it in one
 * place and stops it on teardown.
 */
export function startLiveStats(): () => void {
  let stopped = false;
  let source: EventSource | null = null;
  let watchdog: ReturnType<typeof setInterval> | undefined;
  let poller: ReturnType<typeof setInterval> | undefined;
  let streamRetry: ReturnType<typeof setTimeout> | undefined;
  let fallback: ReturnType<typeof setTimeout> | undefined;
  let refetching = false;
  const controller = new AbortController();

  const refetchSnapshot = async () => {
    if (stopped || refetching) return;
    refetching = true;
    try {
      applySnapshot(await loadStats(controller.signal));
    } catch (error) {
      if (!stopped) setStatus('offline', toMessage(error));
    } finally {
      refetching = false;
    }
  };

  const stopPolling = () => {
    clearInterval(poller);
    poller = undefined;
  };

  /** Start asking, but only if the stream does not come straight back. */
  const scheduleFallback = () => {
    if (stopped || poller || fallback) return;
    fallback = setTimeout(() => {
      fallback = undefined;
      startPolling();
    }, FALLBACK_AFTER_MS);
  };

  const startPolling = () => {
    if (stopped || poller) return;
    setStatus('polling');
    void refetchSnapshot();
    poller = setInterval(() => {
      void refetchSnapshot();
    }, POLL_INTERVAL_MS);
  };

  const closeStream = () => {
    source?.close();
    source = null;
  };

  const connectStream = () => {
    if (stopped) return;
    closeStream();

    const stream = new EventSource(statsStreamUrl());
    source = stream;

    stream.addEventListener('open', () => {
      if (stopped) return;
      clearTimeout(fallback);
      fallback = undefined;
      stopPolling();
      clearTimeout(streamRetry);
      setStatus('live', null);
    });

    stream.addEventListener('tick', (event) => {
      if (stopped) return;
      try {
        const tick = JSON.parse((event as MessageEvent<string>).data) as StatsTick;
        // The server sends seq 0 for a push that only carries an event, so an
        // "applied" toast does not have to wait for the next second.
        if (tick.seq === 0) applyEventOnly(tick);
        else applyTick(tick);
      } catch {
        // A malformed frame is not worth tearing the stream down for.
      }
    });

    // The server says "lagged" when this client fell behind and samples were
    // dropped. Refetching is honest; carrying on would leave a silent hole.
    stream.addEventListener('lagged', () => void refetchSnapshot());

    stream.addEventListener('error', () => {
      if (stopped) return;
      if (stream.readyState === EventSource.CLOSED) {
        // Refused (the 16-stream cap) or blocked by something in between: the
        // browser will not retry this one, so fall back to asking and try the
        // stream again later.
        closeStream();
        startPolling();
        clearTimeout(streamRetry);
        streamRetry = setTimeout(connectStream, STREAM_RETRY_MS);
        return;
      }
      // The browser is already reconnecting. Saying "reconnecting" for the
      // 200 ms that takes would make the banner flicker on every reconnect, so
      // the watchdog decides that, and polling only steps in if the stream
      // stays away.
      scheduleFallback();
    });
  };

  void (async () => {
    try {
      applySnapshot(await loadStats(controller.signal));
    } catch (error) {
      if (!stopped) setStatus('reconnecting', toMessage(error));
    }
    connectStream();
  })();

  // A stream can go quiet without erroring — a proxy in between, a suspended
  // laptop. The chart must say so rather than sit there looking current.
  watchdog = setInterval(() => {
    liveStats.update((state) => {
      if (state.status !== 'live' || state.lastUpdateMs === null) return state;
      if (Date.now() - state.lastUpdateMs < STALE_AFTER_MS) return state;
      return { ...state, status: 'reconnecting' };
    });
  }, WATCHDOG_INTERVAL_MS);

  return () => {
    stopped = true;
    controller.abort();
    closeStream();
    clearInterval(watchdog);
    clearTimeout(streamRetry);
    clearTimeout(fallback);
    stopPolling();
    liveStats.set({ ...initialState });
  };
}
