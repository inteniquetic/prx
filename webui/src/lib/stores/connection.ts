import { derived, get, writable } from 'svelte/store';

/**
 * Whether the admin API is answering.
 *
 * The topbar claims "online", so something has to be able to tell when that
 * stops being true. Every call through $lib/api/admin reports its outcome here,
 * and while the tab is visible a heartbeat keeps asking, so a proxy that went
 * away during a quiet minute is noticed instead of being assumed healthy.
 */
export type ConnectionStatus = 'connecting' | 'online' | 'reconnecting' | 'offline';

export interface ConnectionState {
  status: ConnectionStatus;
  /** Consecutive failures; drives the backoff and the offline threshold. */
  failures: number;
  lastOkAt: number | null;
  lastError: string | null;
}

/** Two misses before calling it offline: one can be a redeploy or a hiccup. */
const OFFLINE_AFTER = 3;
const HEARTBEAT_MS = 15_000;
const MAX_BACKOFF_MS = 60_000;

export const connection = writable<ConnectionState>({
  status: 'connecting',
  failures: 0,
  lastOkAt: null,
  lastError: null
});

export const isOnline = derived(connection, ($connection) => $connection.status === 'online');

export function reportSuccess(): void {
  connection.set({ status: 'online', failures: 0, lastOkAt: Date.now(), lastError: null });
}

export function reportFailure(error: unknown): void {
  connection.update((state) => {
    const failures = state.failures + 1;
    return {
      status: failures >= OFFLINE_AFTER ? 'offline' : 'reconnecting',
      failures,
      lastOkAt: state.lastOkAt,
      lastError: error instanceof Error ? error.message : String(error)
    };
  });
}

/** An aborted request says nothing about the server, so it is not a failure. */
export function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

function nextDelay(failures: number): number {
  if (failures === 0) return HEARTBEAT_MS;
  return Math.min(HEARTBEAT_MS * 2 ** (failures - 1), MAX_BACKOFF_MS);
}

/**
 * Pings `probe` on a schedule that backs off while the server is down and
 * pauses entirely while the tab is hidden — a background tab does not need to
 * keep a dead proxy company.
 */
export function startHeartbeat(probe: () => Promise<unknown>): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  let inFlight = false;

  const schedule = () => {
    if (stopped) return;
    clearTimeout(timer);
    timer = setTimeout(tick, nextDelay(get(connection).failures));
  };

  const tick = async () => {
    if (stopped || inFlight) return;
    if (document.visibilityState === 'hidden') {
      schedule();
      return;
    }

    inFlight = true;
    try {
      await probe();
    } catch {
      // The api client already reported it; the heartbeat only sets the pace.
    } finally {
      inFlight = false;
      schedule();
    }
  };

  const onVisible = () => {
    if (document.visibilityState === 'visible') void tick();
  };

  document.addEventListener('visibilitychange', onVisible);
  schedule();

  return () => {
    stopped = true;
    clearTimeout(timer);
    document.removeEventListener('visibilitychange', onVisible);
  };
}
