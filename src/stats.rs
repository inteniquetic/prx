//! Live statistics for the dashboard (T207).
//!
//! The proxy already counts everything the dashboard wants to show; what it did
//! not have was a way to ask "what is happening *now*" without standing up
//! Prometheus first. This module reads the metric registry once a second, turns
//! the cumulative counters into rates and percentiles, keeps five minutes of
//! them in memory, and pushes each new sample to whoever is watching.
//!
//! Two rules shape the design:
//!
//! * Nothing here runs on the request path. The sampler reads the registry from
//!   the admin runtime, so adding a dashboard costs the proxy one gauge
//!   increment per request and nothing else.
//! * The numbers must be the same ones `/metrics` serves. They come from the
//!   same registry rather than from a second set of counters, so the two cannot
//!   drift apart.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use arc_swap::ArcSwap;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::{
    metrics::{self, MetricsSnapshot, RouteCounters},
    runtime::RuntimeConfig,
};

/// One sample per second, which is as fast as a person can read a dashboard.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
/// Five minutes of history: enough to see a spike start, not enough to matter
/// for memory. Long-term storage is Prometheus' job.
pub const HISTORY_CAPACITY: usize = 300;
/// Per-route numbers are windowed over a minute so a quiet route still has
/// enough requests to say something about its latency.
const ROUTE_WINDOW_SECONDS: u64 = 60;
/// Per-route cumulative snapshots are kept every five seconds rather than every
/// second: with a thousand routes the difference is megabytes.
const ROUTE_BASELINE_INTERVAL: Duration = Duration::from_secs(5);
const ROUTE_BASELINES: usize = (ROUTE_WINDOW_SECONDS as usize / 5) + 1;
/// A dashboard shows a leaderboard, not a route list — the Routes page is the
/// place for all of them.
const MAX_ROUTES_REPORTED: usize = 20;
const MAX_EVENTS: usize = 50;
/// The admin API is a control plane, not a fan-out service.
pub const MAX_STREAM_CLIENTS: usize = 16;
/// A certificate this close to expiry is worth saying out loud.
const CERT_EXPIRY_WARN_SECONDS: i64 = 14 * 24 * 60 * 60;

/// One second of traffic, as rates rather than counters.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sample {
    pub epoch_ms: u64,
    /// Seconds this sample covers; a late tick widens it rather than lying.
    pub window_seconds: f64,
    pub rps: f64,
    pub rps_2xx: f64,
    pub rps_3xx: f64,
    pub rps_4xx: f64,
    pub rps_5xx: f64,
    /// Share of requests in this second that answered 4xx / 5xx, in 0..1.
    pub error_ratio_4xx: f64,
    pub error_ratio_5xx: f64,
    /// `None` while no request finished in the window: a percentile over zero
    /// requests is not zero milliseconds.
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub inflight: u64,
    pub upstreams_healthy: u32,
    pub upstreams_total: u32,
    /// `None` when no route has caching turned on.
    pub cache_hit_ratio: Option<f64>,
}

/// A route's share of the last minute.
#[derive(Debug, Clone, Serialize)]
pub struct RouteStats {
    pub name: String,
    pub requests: u64,
    pub rps: f64,
    pub error_ratio: f64,
    pub requests_4xx: u64,
    pub requests_5xx: u64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub window_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamState {
    Healthy,
    Degraded,
    Down,
    /// Turned off in the config — not a failure, and not counted as one.
    Drained,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpstreamHealth {
    pub addr: String,
    pub state: UpstreamState,
    pub inflight: usize,
    pub ewma_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealth {
    pub name: String,
    pub healthy: u32,
    pub total: u32,
    pub upstreams: Vec<UpstreamHealth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Info,
    Warn,
    Error,
}

/// Something worth a line in the dashboard's event strip.
#[derive(Debug, Clone, Serialize)]
pub struct StatsEvent {
    /// Monotonic, so a reconnecting client can tell what it already has.
    pub id: u64,
    pub epoch_ms: u64,
    pub level: EventLevel,
    /// Machine-readable kind: `config_apply`, `config_reload`, `circuit_open`,
    /// `upstream_down`, `upstream_up`, `cert_expiry`.
    pub kind: String,
    pub message: String,
    /// The route, service or domain the event is about, when there is one.
    pub target: Option<String>,
}

/// What the stream pushes once a second.
///
/// Only `sample` is there every time. Route and service state move slowly, so
/// they ride along only when they changed — a dashboard left open overnight
/// should not cost kilobytes a second.
#[derive(Debug, Clone, Serialize)]
pub struct StatsTick {
    pub seq: u64,
    pub sample: Sample,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<RouteStats>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<Vec<ServiceHealth>>,
    /// Events raised since the previous tick; usually empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<StatsEvent>,
}

/// The first payload a dashboard reads, so the page has a chart before the
/// stream has said anything.
#[derive(Debug, Clone, Serialize)]
pub struct StatsSnapshot {
    pub epoch_ms: u64,
    pub interval_ms: u64,
    pub seq: u64,
    pub uptime_seconds: u64,
    pub sample: Sample,
    pub history: Vec<Sample>,
    pub routes: Vec<RouteStats>,
    pub services: Vec<ServiceHealth>,
    pub events: Vec<StatsEvent>,
    pub stream_clients: usize,
    pub max_stream_clients: usize,
}

#[derive(Default)]
struct HubState {
    seq: u64,
    history: VecDeque<Sample>,
    routes: Vec<RouteStats>,
    services: Vec<ServiceHealth>,
    events: VecDeque<StatsEvent>,
    next_event_id: u64,
}

pub struct StatsHub {
    state: Mutex<HubState>,
    tx: broadcast::Sender<Arc<StatsTick>>,
    clients: AtomicUsize,
    started: Instant,
}

/// Held for as long as a stream is open; the count it guards is what keeps the
/// admin API from fanning out to a hundred tabs.
pub struct ClientGuard {
    hub: &'static StatsHub,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.hub.clients.fetch_sub(1, Ordering::AcqRel);
    }
}

impl StatsHub {
    fn new() -> Self {
        // A slow reader misses samples rather than holding the sampler up; the
        // client notices the gap in the sequence and refetches.
        let (tx, _) = broadcast::channel(64);
        Self {
            state: Mutex::new(HubState::default()),
            tx,
            clients: AtomicUsize::new(0),
            started: Instant::now(),
        }
    }

    /// Takes a stream slot, or returns `None` when the cap is already reached.
    pub fn subscribe(&'static self) -> Option<(broadcast::Receiver<Arc<StatsTick>>, ClientGuard)> {
        let mut current = self.clients.load(Ordering::Acquire);
        loop {
            if current >= MAX_STREAM_CLIENTS {
                return None;
            }
            match self.clients.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some((self.tx.subscribe(), ClientGuard { hub: self })),
                Err(actual) => current = actual,
            }
        }
    }

    pub fn client_count(&self) -> usize {
        self.clients.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        StatsSnapshot {
            epoch_ms: now_epoch_ms(),
            interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            seq: state.seq,
            uptime_seconds: self.started.elapsed().as_secs(),
            sample: state.history.back().cloned().unwrap_or_default(),
            history: state.history.iter().cloned().collect(),
            routes: state.routes.clone(),
            services: state.services.clone(),
            events: state.events.iter().rev().cloned().collect(),
            stream_clients: self.client_count(),
            max_stream_clients: MAX_STREAM_CLIENTS,
        }
    }

    fn push_event(
        &self,
        level: EventLevel,
        kind: &str,
        message: String,
        target: Option<String>,
    ) -> StatsEvent {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.next_event_id += 1;
        let event = StatsEvent {
            id: state.next_event_id,
            epoch_ms: now_epoch_ms(),
            level,
            kind: kind.to_string(),
            message,
            target,
        };
        state.events.push_back(event.clone());
        while state.events.len() > MAX_EVENTS {
            state.events.pop_front();
        }
        event
    }
}

pub fn hub() -> &'static StatsHub {
    static HUB: OnceLock<StatsHub> = OnceLock::new();
    HUB.get_or_init(StatsHub::new)
}

/// Records an event for the dashboard's event strip.
///
/// Config applies and reloads call this directly; upstream and certificate
/// events are derived by the sampler from the metrics it already reads, so
/// nothing has to be instrumented twice.
pub fn record_event(level: EventLevel, kind: &str, message: impl Into<String>) {
    let event = hub().push_event(level, kind, message.into(), None);
    broadcast_event(event);
}

pub fn record_event_for(
    level: EventLevel,
    kind: &str,
    message: impl Into<String>,
    target: impl Into<String>,
) {
    let event = hub().push_event(level, kind, message.into(), Some(target.into()));
    broadcast_event(event);
}

/// Events that arrive between ticks still reach open dashboards immediately:
/// "config applied" a second late reads as "nothing happened".
fn broadcast_event(event: StatsEvent) {
    let hub = hub();
    if hub.tx.receiver_count() == 0 {
        return;
    }
    let sample = {
        let state = hub.state.lock().unwrap_or_else(|err| err.into_inner());
        state.history.back().cloned().unwrap_or_default()
    };
    let _ = hub.tx.send(Arc::new(StatsTick {
        seq: 0,
        sample,
        routes: None,
        services: None,
        events: vec![event],
    }));
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

/// Cumulative state the next tick is measured against.
struct Sampler {
    active_config: Arc<ArcSwap<RuntimeConfig>>,
    last: Option<(Instant, MetricsSnapshot)>,
    /// `(taken_at, per-route cumulative counters)`, oldest first.
    route_baselines: VecDeque<(Instant, HashMap<String, RouteCounters>)>,
    last_route_baseline: Option<Instant>,
    upstream_states: HashMap<(String, String), UpstreamState>,
    /// Domains already warned about, so one expiring certificate does not fill
    /// the event strip once a second.
    cert_warned: HashMap<String, i64>,
    seen_services: bool,
}

/// Starts the once-a-second sampler. Safe to call more than once; only the
/// first call starts a task.
pub fn spawn_sampler(active_config: Arc<ArcSwap<RuntimeConfig>>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    tokio::spawn(async move {
        let mut sampler = Sampler {
            active_config,
            last: None,
            route_baselines: VecDeque::new(),
            last_route_baseline: None,
            upstream_states: HashMap::new(),
            cert_warned: HashMap::new(),
            seen_services: false,
        };

        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        // A tick that ran late must not be made up for by a burst of catch-up
        // ticks; each sample is a real second of traffic or it is nothing.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            sampler.tick();
        }
    });
}

impl Sampler {
    fn tick(&mut self) {
        let now = Instant::now();
        let snapshot = metrics::snapshot();

        let Some((previous_at, previous)) = self.last.take() else {
            // The first tick has nothing to subtract from; it only sets the
            // baseline so the second one is a real rate.
            self.route_baselines
                .push_back((now, snapshot.routes.clone()));
            self.last_route_baseline = Some(now);
            self.last = Some((now, snapshot));
            return;
        };

        let elapsed = now.duration_since(previous_at).as_secs_f64().max(0.001);
        let sample = build_sample(&previous, &snapshot, elapsed);

        // Route windows move on a slower clock than the sample does.
        let rotate = self
            .last_route_baseline
            .is_none_or(|at| now.duration_since(at) >= ROUTE_BASELINE_INTERVAL);
        let routes = if rotate {
            self.route_baselines
                .push_back((now, snapshot.routes.clone()));
            while self.route_baselines.len() > ROUTE_BASELINES {
                self.route_baselines.pop_front();
            }
            self.last_route_baseline = Some(now);
            Some(self.route_window(&snapshot, now))
        } else {
            None
        };

        let (services, health_events) = self.service_health();
        let mut events = health_events;
        events.extend(self.certificate_events(&snapshot));

        let mut sample = sample;
        let (healthy, total) = services.iter().fold((0, 0), |(healthy, total), service| {
            (healthy + service.healthy, total + service.total)
        });
        sample.upstreams_healthy = healthy;
        sample.upstreams_total = total;

        let (seq, services_payload) = {
            let hub = hub();
            let mut state = hub.state.lock().unwrap_or_else(|err| err.into_inner());
            state.seq += 1;
            state.history.push_back(sample.clone());
            while state.history.len() > HISTORY_CAPACITY {
                state.history.pop_front();
            }
            if let Some(routes) = &routes {
                state.routes = routes.clone();
            }
            // Health state moves in steps, so it rides along only when it
            // actually stepped; in-flight counts changing is not a step.
            let changed = state.services != services;
            if changed {
                state.services = services;
            }
            (state.seq, changed.then(|| state.services.clone()))
        };

        // Events raised here go into the ring first so a client that refetches
        // after a gap sees the same list the stream pushed.
        let events: Vec<StatsEvent> = events
            .into_iter()
            .map(|(level, kind, message, target)| hub().push_event(level, &kind, message, target))
            .collect();

        self.last = Some((now, snapshot));

        let hub = hub();
        if hub.tx.receiver_count() > 0 {
            let _ = hub.tx.send(Arc::new(StatsTick {
                seq,
                sample,
                routes,
                services: services_payload,
                events,
            }));
        }
    }

    /// Per-route numbers over the oldest baseline still inside the window.
    fn route_window(&self, snapshot: &MetricsSnapshot, now: Instant) -> Vec<RouteStats> {
        let window = Duration::from_secs(ROUTE_WINDOW_SECONDS);
        let baseline = self
            .route_baselines
            .iter()
            .find(|(at, _)| now.duration_since(*at) <= window)
            .or_else(|| self.route_baselines.front());

        let Some((baseline_at, baseline)) = baseline else {
            return Vec::new();
        };
        let elapsed = now.duration_since(*baseline_at).as_secs_f64();
        if elapsed <= 0.0 {
            return Vec::new();
        }

        let mut routes: Vec<RouteStats> = snapshot
            .routes
            .iter()
            .map(|(name, counters)| {
                let delta = RouteDelta::between(baseline.get(name), counters);
                let errors = delta.status_4xx + delta.status_5xx;
                RouteStats {
                    name: name.clone(),
                    requests: delta.total,
                    rps: delta.total as f64 / elapsed,
                    error_ratio: if delta.total == 0 {
                        0.0
                    } else {
                        errors as f64 / delta.total as f64
                    },
                    requests_4xx: delta.status_4xx,
                    requests_5xx: delta.status_5xx,
                    p50_ms: delta.quantile(0.50),
                    p95_ms: delta.quantile(0.95),
                    p99_ms: delta.quantile(0.99),
                    window_seconds: elapsed,
                }
            })
            .filter(|route| route.requests > 0)
            .collect();

        routes.sort_by(|a, b| b.requests.cmp(&a.requests).then(a.name.cmp(&b.name)));
        routes.truncate(MAX_ROUTES_REPORTED);
        routes
    }

    /// Upstream health straight from the running config — the same source the
    /// Services page reads, so the two pages cannot disagree.
    #[allow(clippy::type_complexity)]
    fn service_health(
        &mut self,
    ) -> (
        Vec<ServiceHealth>,
        Vec<(EventLevel, String, String, Option<String>)>,
    ) {
        let config = self.active_config.load();
        let mut services = Vec::with_capacity(config.service_count());
        let mut events = Vec::new();
        let mut seen = HashMap::new();

        for idx in 0..config.service_count() {
            let Some(service) = config.service(idx) else {
                continue;
            };
            let mut upstreams = Vec::with_capacity(service.upstreams.len());
            let mut healthy = 0;
            let mut total = 0;

            for upstream in &service.upstreams {
                // An open circuit and a failing probe are the same thing to a
                // dashboard: no traffic is going there.
                let state = if !upstream.enabled {
                    UpstreamState::Drained
                } else if upstream.is_circuit_open() || !upstream.is_probe_healthy() {
                    UpstreamState::Down
                } else if upstream.consecutive_failures() > 0 {
                    UpstreamState::Degraded
                } else {
                    UpstreamState::Healthy
                };

                if state != UpstreamState::Drained {
                    total += 1;
                    if state == UpstreamState::Healthy {
                        healthy += 1;
                    }
                }

                let key = (service.name.clone(), upstream.addr.clone());
                let previous = self.upstream_states.get(&key).copied();
                if self.seen_services && previous.is_some_and(|before| before != state) {
                    let target = format!("{}/{}", service.name, upstream.addr);
                    match state {
                        UpstreamState::Down if upstream.is_circuit_open() => events.push((
                            EventLevel::Error,
                            "circuit_open".to_string(),
                            format!("Circuit opened for {target}"),
                            Some(service.name.clone()),
                        )),
                        UpstreamState::Down => events.push((
                            EventLevel::Error,
                            "upstream_down".to_string(),
                            format!("{target} stopped answering health checks"),
                            Some(service.name.clone()),
                        )),
                        UpstreamState::Healthy => events.push((
                            EventLevel::Info,
                            "upstream_up".to_string(),
                            format!("{target} is healthy again"),
                            Some(service.name.clone()),
                        )),
                        UpstreamState::Degraded => events.push((
                            EventLevel::Warn,
                            "upstream_degraded".to_string(),
                            format!("{target} is failing intermittently"),
                            Some(service.name.clone()),
                        )),
                        UpstreamState::Drained => events.push((
                            EventLevel::Info,
                            "upstream_drained".to_string(),
                            format!("{target} was drained"),
                            Some(service.name.clone()),
                        )),
                    }
                }
                seen.insert(key, state);

                upstreams.push(UpstreamHealth {
                    addr: upstream.addr.clone(),
                    state,
                    inflight: upstream.inflight(),
                    ewma_ms: upstream.ewma_us() as f64 / 1000.0,
                });
            }

            services.push(ServiceHealth {
                name: service.name.clone(),
                healthy,
                total,
                upstreams,
            });
        }

        self.upstream_states = seen;
        self.seen_services = true;
        (services, events)
    }

    #[allow(clippy::type_complexity)]
    fn certificate_events(
        &mut self,
        snapshot: &MetricsSnapshot,
    ) -> Vec<(EventLevel, String, String, Option<String>)> {
        let mut events = Vec::new();
        for (domain, seconds) in &snapshot.cert_expiry_seconds {
            if *seconds > CERT_EXPIRY_WARN_SECONDS {
                self.cert_warned.remove(domain);
                continue;
            }
            let days = (*seconds).div_euclid(24 * 60 * 60);
            // Warn once, then again only when it crosses into another day.
            if self.cert_warned.get(domain).copied() == Some(days) {
                continue;
            }
            self.cert_warned.insert(domain.clone(), days);
            events.push((
                if days <= 3 {
                    EventLevel::Error
                } else {
                    EventLevel::Warn
                },
                "cert_expiry".to_string(),
                if days <= 0 {
                    format!("The certificate for {domain} expires today")
                } else {
                    format!("The certificate for {domain} expires in {days} day(s)")
                },
                Some(domain.clone()),
            ));
        }
        events
    }
}

/// Equality that ignores the numbers which move every second: two health
/// snapshots are "the same" when the same upstreams are in the same state, even
/// if in-flight went from 3 to 4.
impl PartialEq for ServiceHealth {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.healthy == other.healthy
            && self.total == other.total
            && self.upstreams.len() == other.upstreams.len()
            && self
                .upstreams
                .iter()
                .zip(&other.upstreams)
                .all(|(a, b)| a.addr == b.addr && a.state == b.state)
    }
}

/// What changed for one route between two cumulative snapshots.
#[derive(Debug, Default)]
struct RouteDelta {
    total: u64,
    status_2xx: u64,
    status_3xx: u64,
    status_4xx: u64,
    status_5xx: u64,
    buckets: Vec<u64>,
    latency_count: u64,
}

impl RouteDelta {
    fn between(before: Option<&RouteCounters>, after: &RouteCounters) -> Self {
        let zero = RouteCounters::default();
        let before = before.unwrap_or(&zero);
        // A counter that went backwards means its label was dropped and
        // recreated; count that second as quiet rather than printing a negative
        // rate or a spike that never happened.
        let sub = |after: u64, before: u64| after.saturating_sub(before);
        let buckets = after
            .latency_buckets
            .iter()
            .enumerate()
            .map(|(idx, count)| {
                sub(
                    *count,
                    before.latency_buckets.get(idx).copied().unwrap_or(0),
                )
            })
            .collect();

        let status_2xx = sub(after.status_2xx, before.status_2xx);
        let status_3xx = sub(after.status_3xx, before.status_3xx);
        let status_4xx = sub(after.status_4xx, before.status_4xx);
        let status_5xx = sub(after.status_5xx, before.status_5xx);
        let status_other = sub(after.status_other, before.status_other);

        Self {
            total: status_2xx + status_3xx + status_4xx + status_5xx + status_other,
            status_2xx,
            status_3xx,
            status_4xx,
            status_5xx,
            buckets,
            latency_count: sub(after.latency_count, before.latency_count),
        }
    }

    fn quantile(&self, q: f64) -> Option<f64> {
        quantile_ms(
            metrics::latency_bucket_bounds(),
            &self.buckets,
            self.latency_count,
            q,
        )
    }
}

/// The same interpolation Prometheus' `histogram_quantile` does, over the
/// buckets that filled up during the window.
fn quantile_ms(bounds: &[f64], buckets: &[u64], count: u64, q: f64) -> Option<f64> {
    if count == 0 || buckets.is_empty() {
        return None;
    }
    let rank = q * count as f64;
    let mut lower_bound = 0.0;
    let mut lower_count = 0u64;

    for (idx, cumulative) in buckets.iter().enumerate() {
        let upper_bound = match bounds.get(idx) {
            Some(bound) => *bound,
            // The +Inf bucket: everything left is slower than the last bound,
            // and there is no upper edge to interpolate towards.
            None => return Some(lower_bound),
        };
        if (*cumulative as f64) >= rank {
            let in_bucket = cumulative.saturating_sub(lower_count);
            if in_bucket == 0 {
                return Some(upper_bound);
            }
            let position = (rank - lower_count as f64) / in_bucket as f64;
            return Some(lower_bound + (upper_bound - lower_bound) * position.clamp(0.0, 1.0));
        }
        lower_bound = upper_bound;
        lower_count = *cumulative;
    }

    // Everything above the last bucket edge: report the edge rather than
    // inventing a number past it.
    Some(bounds.last().copied().unwrap_or(lower_bound))
}

fn build_sample(before: &MetricsSnapshot, after: &MetricsSnapshot, elapsed: f64) -> Sample {
    let mut totals = RouteDelta::default();
    let bucket_len = metrics::latency_bucket_bounds().len();
    totals.buckets = vec![0; bucket_len];

    for (name, counters) in &after.routes {
        let delta = RouteDelta::between(before.routes.get(name), counters);
        totals.total += delta.total;
        totals.status_2xx += delta.status_2xx;
        totals.status_3xx += delta.status_3xx;
        totals.status_4xx += delta.status_4xx;
        totals.status_5xx += delta.status_5xx;
        totals.latency_count += delta.latency_count;
        for (idx, value) in delta.buckets.iter().enumerate() {
            if let Some(slot) = totals.buckets.get_mut(idx) {
                *slot += value;
            }
        }
    }

    let requests = totals.total as f64;
    let cache_lookups = after.cache_lookups.saturating_sub(before.cache_lookups);
    let cache_hits = after.cache_hits.saturating_sub(before.cache_hits);

    Sample {
        epoch_ms: now_epoch_ms(),
        window_seconds: elapsed,
        rps: requests / elapsed,
        rps_2xx: totals.status_2xx as f64 / elapsed,
        rps_3xx: totals.status_3xx as f64 / elapsed,
        rps_4xx: totals.status_4xx as f64 / elapsed,
        rps_5xx: totals.status_5xx as f64 / elapsed,
        error_ratio_4xx: if requests == 0.0 {
            0.0
        } else {
            totals.status_4xx as f64 / requests
        },
        error_ratio_5xx: if requests == 0.0 {
            0.0
        } else {
            totals.status_5xx as f64 / requests
        },
        p50_ms: totals.quantile(0.50),
        p95_ms: totals.quantile(0.95),
        p99_ms: totals.quantile(0.99),
        inflight: after.inflight.max(0) as u64,
        upstreams_healthy: 0,
        upstreams_total: 0,
        cache_hit_ratio: (cache_lookups > 0).then(|| cache_hits as f64 / cache_lookups as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(status_2xx: u64, status_5xx: u64, buckets: &[u64]) -> RouteCounters {
        RouteCounters {
            status_2xx,
            status_5xx,
            latency_buckets: buckets.to_vec(),
            latency_count: buckets.last().copied().unwrap_or(0),
            ..RouteCounters::default()
        }
    }

    #[test]
    fn rates_are_per_second_not_per_tick() {
        let mut before = MetricsSnapshot::default();
        before
            .routes
            .insert("api".to_string(), counters(100, 0, &[100]));
        let mut after = MetricsSnapshot::default();
        after
            .routes
            .insert("api".to_string(), counters(160, 20, &[180]));

        let sample = build_sample(&before, &after, 2.0);
        assert_eq!(sample.rps, 40.0);
        assert_eq!(sample.rps_5xx, 10.0);
        assert!((sample.error_ratio_5xx - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_counter_that_went_backwards_reads_as_a_quiet_second() {
        let mut before = MetricsSnapshot::default();
        before
            .routes
            .insert("api".to_string(), counters(5_000, 0, &[5_000]));
        let mut after = MetricsSnapshot::default();
        after.routes.insert("api".to_string(), counters(7, 0, &[7]));

        // A label that was dropped and came back is the only way this happens
        // in one process. Reporting zero for that second beats reporting a
        // negative rate, or a spike of thousands of requests that never ran.
        let sample = build_sample(&before, &after, 1.0);
        assert_eq!(sample.rps, 0.0);
    }

    #[test]
    fn percentiles_are_none_without_traffic() {
        let sample = build_sample(
            &MetricsSnapshot::default(),
            &MetricsSnapshot::default(),
            1.0,
        );
        assert_eq!(sample.p99_ms, None);
        assert_eq!(sample.rps, 0.0);
        assert_eq!(sample.cache_hit_ratio, None);
    }

    #[test]
    fn quantiles_interpolate_inside_the_bucket() {
        // Bounds: 1, 2.5, 5, 10, ... Ten requests, all between 2.5 ms and 5 ms.
        let bounds = metrics::latency_bucket_bounds();
        let mut buckets = vec![0u64; bounds.len()];
        for slot in buckets.iter_mut().skip(2) {
            *slot = 10;
        }

        let p50 = quantile_ms(bounds, &buckets, 10, 0.5).expect("p50");
        assert!(p50 > 2.5 && p50 < 5.0, "p50 landed at {p50}");

        let p99 = quantile_ms(bounds, &buckets, 10, 0.99).expect("p99");
        assert!(p99 > p50 && p99 <= 5.0, "p99 landed at {p99}");
    }

    #[test]
    fn cache_hit_ratio_covers_only_the_window() {
        let before = MetricsSnapshot {
            cache_hits: 10,
            cache_lookups: 20,
            ..MetricsSnapshot::default()
        };
        let after = MetricsSnapshot {
            cache_hits: 13,
            cache_lookups: 24,
            ..MetricsSnapshot::default()
        };
        let sample = build_sample(&before, &after, 1.0);
        assert_eq!(sample.cache_hit_ratio, Some(0.75));
    }

    #[test]
    fn events_keep_only_the_most_recent() {
        let hub = hub();
        for index in 0..(MAX_EVENTS + 5) {
            hub.push_event(
                EventLevel::Info,
                "config_apply",
                format!("event {index}"),
                None,
            );
        }
        let snapshot = hub.snapshot();
        assert_eq!(snapshot.events.len(), MAX_EVENTS);
        // Newest first, so the strip does not have to reverse it.
        assert!(snapshot.events[0].id > snapshot.events[1].id);
    }
}
