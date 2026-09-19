use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rand::Rng;

use crate::{
    cache::ResponseCache,
    config::{
        CacheConfig, ConcurrencyLimitConfig, HealthCheckConfig, LbStrategy, PrxConfig,
        RateLimitKey, StickyConfig, UpstreamH2,
    },
    headers::CompiledHeaderRules,
    limiter::{ConcurrencyLimiter, RateLimiter},
    router::{IndexedRoute, RouteIndex, RouteMatch, method_mask},
};

#[derive(Debug)]
pub struct RuntimeConfig {
    routes: Vec<RouteRuntime>,
    services: Vec<ServiceRuntime>,
    index: RouteIndex,
}

impl RuntimeConfig {
    pub fn from_config(config: PrxConfig) -> Self {
        // Build services first with their upstreams
        let services = config
            .services
            .into_iter()
            .map(ServiceRuntime::from_config)
            .collect::<Vec<_>>();

        // Build a name-to-index map for service resolution
        let service_index: std::collections::HashMap<String, usize> = services
            .iter()
            .enumerate()
            .map(|(idx, svc)| (svc.name.clone(), idx))
            .collect();

        // Global header rules are compiled once and merged into each route, so
        // the request path applies a single list instead of two (T106).
        let global_request_headers = CompiledHeaderRules::compile(&config.headers.request);
        let global_response_headers = CompiledHeaderRules::compile(&config.headers.response);

        // Routes keep their config order: the index encodes precedence, so
        // route indices stay stable and predictable for logs and the admin API.
        let routes = config
            .routes
            .into_iter()
            .map(|route| {
                RouteRuntime::from_config(
                    route,
                    &service_index,
                    &global_request_headers,
                    &global_response_headers,
                )
            })
            .collect::<Vec<_>>();

        let index = RouteIndex::build(routes.iter().map(|route| IndexedRoute {
            host: route.host.as_deref(),
            path_prefix: route.path_prefix.as_str(),
            methods: route.methods,
            is_default: route.is_default,
            enabled: route.enabled,
        }));

        Self {
            routes,
            services,
            index,
        }
    }

    /// Matches a request against the route index.
    ///
    /// `host` is the raw Host header; `method` is the request method, or `None`
    /// to ignore method filtering. Neither argument is allocated from.
    pub fn select(&self, host: &str, path: &str, method: Option<&str>) -> RouteMatch {
        match normalize_host(host) {
            Cow::Borrowed(host) => self.index.select(host, path, method),
            Cow::Owned(host) => self.index.select(&host, path, method),
        }
    }

    /// Convenience wrapper that ignores method filtering.
    pub fn select_route(&self, host: &str, path: &str) -> Option<usize> {
        match self.select(host, path, None) {
            RouteMatch::Matched(idx) => Some(idx),
            RouteMatch::MethodNotAllowed | RouteMatch::NotFound => None,
        }
    }

    pub fn route(&self, idx: usize) -> Option<&RouteRuntime> {
        self.routes.get(idx)
    }

    pub fn service(&self, idx: usize) -> Option<&ServiceRuntime> {
        self.services.get(idx)
    }

    /// Number of routes, so callers can walk them by index.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// Number of services, so the health checker can walk them by index.
    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    pub fn is_ready(&self) -> bool {
        self.services
            .iter()
            .all(ServiceRuntime::has_available_upstream)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerRuntime {
    enabled: bool,
    consecutive_failures: usize,
    open_ms: u64,
}

impl CircuitBreakerRuntime {
    fn from_config(config: &crate::config::CircuitBreakerConfig) -> Self {
        Self {
            enabled: config.enabled,
            consecutive_failures: config.consecutive_failures.max(1),
            open_ms: config.open_ms.max(1),
        }
    }
}

#[derive(Debug)]
pub struct RouteRuntime {
    /// `Arc<str>` so cloning the name per request is a refcount bump, not an
    /// allocation (T101).
    pub name: Arc<str>,
    pub host: Option<String>,
    pub path_prefix: String,
    pub methods: crate::router::MethodMask,
    pub is_default: bool,
    /// Mirrors the config flag. A disabled route is kept here so the admin API
    /// can still list it; it is simply absent from the index.
    pub enabled: bool,
    pub service_idx: usize,
    /// Global rules merged with the route's own, compiled once per reload.
    pub request_headers: CompiledHeaderRules,
    pub response_headers: CompiledHeaderRules,
    /// `None` when caching is off for this route.
    pub cache: Option<RouteCache>,
    /// `None` when rate limiting is off for this route.
    pub rate_limit: Option<RouteRateLimit>,
    pub concurrency_limit: ConcurrencyLimitConfig,
    pub concurrency: ConcurrencyLimiter,
}

/// A route's response cache and the settings it was built from.
#[derive(Debug)]
pub struct RouteCache {
    pub config: CacheConfig,
    pub store: ResponseCache,
    /// Header names that take part in the cache key, parsed once.
    pub vary_headers: Vec<String>,
}

/// A route's compiled rate limit.
#[derive(Debug)]
pub struct RouteRateLimit {
    pub key: RateLimitKey,
    pub limiter: RateLimiter,
    pub response_status: u16,
    pub retry_after: bool,
}

impl RouteRuntime {
    fn from_config(
        config: crate::config::RouteConfig,
        service_index: &std::collections::HashMap<String, usize>,
        global_request_headers: &CompiledHeaderRules,
        global_response_headers: &CompiledHeaderRules,
    ) -> Self {
        let host = config
            .host
            .as_deref()
            .map(|host| normalize_host_pattern(host).into_owned());
        let service_idx = service_index
            .get(&config.service)
            .copied()
            .expect("route references a service that was not found in service_index");

        let request_headers = CompiledHeaderRules::merge(
            global_request_headers,
            &CompiledHeaderRules::compile(&config.request_headers),
        );
        let response_headers = CompiledHeaderRules::merge(
            global_response_headers,
            &CompiledHeaderRules::compile(&config.response_headers),
        );

        // Compiled once per reload: the request path only hashes a key and
        // takes a token.
        let rate_limit = config
            .rate_limit
            .enabled
            .then(|| {
                RateLimitKey::parse(&config.rate_limit.key).map(|key| RouteRateLimit {
                    key,
                    limiter: RateLimiter::new(
                        config.rate_limit.requests_per_second,
                        if config.rate_limit.burst == 0 {
                            config.rate_limit.requests_per_second
                        } else {
                            config.rate_limit.burst
                        },
                        config.rate_limit.entry_ttl_ms,
                        config.rate_limit.max_entries,
                    ),
                    response_status: config.rate_limit.response_status,
                    retry_after: config.rate_limit.retry_after,
                })
            })
            .flatten();

        let cache = config.cache.enabled.then(|| {
            let vary_headers = config
                .cache
                .vary_headers
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .collect();
            RouteCache {
                store: ResponseCache::new(
                    std::time::Duration::from_millis(config.cache.ttl_ms),
                    config.cache.max_entries,
                    config.cache.max_bytes,
                    std::time::Duration::from_millis(config.cache.coalesce_wait_ms),
                ),
                vary_headers,
                config: config.cache,
            }
        });

        Self {
            name: config.name.into(),
            host,
            path_prefix: config.path_prefix,
            methods: method_mask(&config.methods),
            is_default: config.is_default,
            enabled: config.enabled,
            service_idx,
            request_headers,
            response_headers,
            cache,
            rate_limit,
            concurrency_limit: config.concurrency_limit,
            concurrency: ConcurrencyLimiter::default(),
        }
    }
}

#[derive(Debug)]
pub struct ServiceRuntime {
    pub name: String,
    pub lb: LbStrategy,
    pub upstream_h2: UpstreamH2,
    pub max_retries: usize,
    pub retry_backoff_ms: u64,
    pub retry_idempotent_only: bool,
    pub request_timeout_ms: u64,
    pub retry_budget: RetryBudget,
    pub health_check: HealthCheckConfig,
    pub sticky: StickyConfig,
    pub circuit_breaker: CircuitBreakerRuntime,
    pub upstreams: Vec<UpstreamRuntime>,
    ring: Vec<usize>,
    rr_cursor: Arc<AtomicUsize>,
}

impl ServiceRuntime {
    fn from_config(config: crate::config::ServiceConfig) -> Self {
        let circuit_breaker = CircuitBreakerRuntime::from_config(&config.circuit_breaker);
        let upstreams = config
            .upstreams
            .into_iter()
            .map(UpstreamRuntime::from_config)
            .collect::<Vec<_>>();
        let ring = build_selection_ring(&upstreams);

        Self {
            name: config.name,
            lb: config.lb,
            upstream_h2: config.upstream_h2,
            max_retries: config.max_retries,
            retry_backoff_ms: config.retry_backoff_ms,
            retry_idempotent_only: config.retry_idempotent_only,
            request_timeout_ms: config.request_timeout_ms,
            retry_budget: RetryBudget::new(
                config.retry_budget_ratio,
                config.retry_budget_min_per_window,
                config.retry_budget_window_ms,
            ),
            health_check: config.health_check,
            sticky: config.sticky,
            circuit_breaker,
            upstreams,
            ring,
            rr_cursor: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn next_upstream(
        &self,
        hash_seed: u64,
        attempted: &[usize],
    ) -> Option<(usize, &UpstreamRuntime)> {
        if self.upstreams.is_empty() || self.ring.is_empty() {
            return None;
        }

        let chosen_idx = match self.lb {
            LbStrategy::RoundRobin => self.select_round_robin(attempted),
            LbStrategy::Random => self.select_random(attempted),
            LbStrategy::Hash => self.select_hash(hash_seed, attempted),
            LbStrategy::LeastConn => {
                self.select_power_of_two(attempted, |upstream| upstream.inflight() as u64)
            }
            LbStrategy::P2cEwma => self.select_power_of_two(attempted, UpstreamRuntime::load_score),
        }?;

        self.upstreams
            .get(chosen_idx)
            .map(|upstream| (chosen_idx, upstream))
    }

    /// What `next_upstream` would return, without changing anything.
    ///
    /// The route tester has to answer "where would this request go?" without
    /// nudging the live round-robin cursor, so this reads the cursor instead of
    /// advancing it. Strategies that draw at random have no answer to give and
    /// return `None`; the caller says so rather than inventing one.
    pub fn peek_upstream(&self, hash_seed: u64) -> Option<(usize, &UpstreamRuntime)> {
        if self.upstreams.is_empty() || self.ring.is_empty() {
            return None;
        }

        let chosen_idx = match self.lb {
            LbStrategy::RoundRobin => {
                self.select_from_ring(self.rr_cursor.load(Ordering::Relaxed), &[])
            }
            LbStrategy::Hash => self.select_hash(hash_seed, &[]),
            LbStrategy::Random | LbStrategy::LeastConn | LbStrategy::P2cEwma => None,
        }?;

        self.upstreams
            .get(chosen_idx)
            .map(|upstream| (chosen_idx, upstream))
    }

    fn select_round_robin(&self, attempted: &[usize]) -> Option<usize> {
        let start = self.rr_cursor.fetch_add(1, Ordering::Relaxed);
        self.select_from_ring(start, attempted)
    }

    fn select_random(&self, attempted: &[usize]) -> Option<usize> {
        let mut rng = rand::rng();
        let random_start = rng.random_range(0..self.ring.len());
        self.select_from_ring(random_start, attempted)
    }

    fn select_hash(&self, hash_seed: u64, attempted: &[usize]) -> Option<usize> {
        let base = (hash_seed as usize) % self.ring.len();
        self.select_from_ring(base, attempted)
    }

    /// Power of two choices: sample two candidates from the weighted ring and
    /// keep the one with the lower score. Constant work per request, and it
    /// avoids the herd effect of everyone picking whichever upstream currently
    /// looks best.
    ///
    /// The two draws are made without replacement. With replacement, a service
    /// with two upstreams would send a quarter of its traffic to the worse one
    /// simply because both draws landed on it - and two or three upstreams is
    /// the common case. The retry count is bounded so this stays O(1).
    fn select_power_of_two(
        &self,
        attempted: &[usize],
        score: impl Fn(&UpstreamRuntime) -> u64,
    ) -> Option<usize> {
        const DRAW_ATTEMPTS: usize = 4;

        let mut rng = rand::rng();
        let first = self.select_from_ring(rng.random_range(0..self.ring.len()), attempted)?;

        let mut second = first;
        for _ in 0..DRAW_ATTEMPTS {
            let candidate =
                self.select_from_ring(rng.random_range(0..self.ring.len()), attempted)?;
            if candidate != first {
                second = candidate;
                break;
            }
        }
        if first == second {
            return Some(first);
        }

        let first_score = self.upstreams.get(first).map(&score)?;
        let second_score = self.upstreams.get(second).map(&score)?;
        Some(if first_score <= second_score {
            first
        } else {
            second
        })
    }

    /// Picks an upstream from the hash ring, regardless of the configured
    /// strategy. Affinity modes that hash a key (`client_ip`, `header`) need
    /// this: routing them through the service's own strategy would ignore the
    /// key entirely and spread the client across upstreams.
    pub fn select_by_hash(&self, hash: u64, attempted: &[usize]) -> Option<usize> {
        if self.ring.is_empty() {
            return None;
        }
        self.select_hash(hash, attempted)
    }

    /// Returns the upstream a sticky key points at, when it is still usable.
    pub fn select_sticky(&self, addr_hash: u64, attempted: &[usize]) -> Option<usize> {
        let now_ms = now_epoch_ms();
        self.upstreams
            .iter()
            .position(|upstream| {
                upstream.addr_hash() == addr_hash && upstream.is_available_at(now_ms)
            })
            .filter(|idx| !attempted.contains(idx))
    }

    fn select_from_ring(&self, start: usize, attempted: &[usize]) -> Option<usize> {
        let now_ms = now_epoch_ms();
        for offset in 0..self.ring.len() {
            let candidate = self.ring[(start + offset) % self.ring.len()];
            if !attempted.contains(&candidate)
                && self
                    .upstreams
                    .get(candidate)
                    .is_some_and(|upstream| upstream.is_available_at(now_ms))
            {
                return Some(candidate);
            }
        }
        None
    }

    pub fn has_available_upstream(&self) -> bool {
        let now_ms = now_epoch_ms();
        self.upstreams
            .iter()
            .any(|upstream| upstream.is_available_at(now_ms))
    }

    pub fn mark_upstream_failure(&self, upstream_idx: usize) -> bool {
        let Some(upstream) = self.upstreams.get(upstream_idx) else {
            return false;
        };
        upstream.mark_failure(&self.circuit_breaker)
    }

    pub fn mark_upstream_success(&self, upstream_idx: usize) {
        if let Some(upstream) = self.upstreams.get(upstream_idx) {
            upstream.mark_success();
        }
    }
}

/// Caps how much extra load retries may add while an upstream is failing.
///
/// Without it, an upstream that starts failing gets `1 + max_retries` times its
/// usual traffic at the worst possible moment. The budget is measured over a
/// sliding window: retries are allowed while they stay under
/// `ratio * successes`, with a small floor so an idle or freshly started
/// service can still retry.
#[derive(Debug)]
pub struct RetryBudget {
    ratio: f64,
    min_per_window: u64,
    window_ms: u64,
    /// Start of the current window, in epoch milliseconds.
    window_start_ms: AtomicU64,
    successes: AtomicU64,
    retries: AtomicU64,
}

impl RetryBudget {
    fn new(ratio: f64, min_per_window: u64, window_ms: u64) -> Self {
        Self {
            ratio,
            min_per_window,
            window_ms: window_ms.max(1),
            window_start_ms: AtomicU64::new(now_epoch_ms()),
            successes: AtomicU64::new(0),
            retries: AtomicU64::new(0),
        }
    }

    /// Disabled budgets impose no limit beyond `max_retries`.
    pub fn is_enabled(&self) -> bool {
        self.ratio > 0.0
    }

    fn roll_window(&self) {
        let now = now_epoch_ms();
        let start = self.window_start_ms.load(Ordering::Relaxed);
        if now.saturating_sub(start) < self.window_ms {
            return;
        }
        // Whoever wins the swap resets the counters; the others just carry on.
        if self
            .window_start_ms
            .compare_exchange(start, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            self.successes.store(0, Ordering::Relaxed);
            self.retries.store(0, Ordering::Relaxed);
        }
    }

    pub fn record_success(&self) {
        if !self.is_enabled() {
            return;
        }
        self.roll_window();
        self.successes.fetch_add(1, Ordering::Relaxed);
    }

    /// Takes one retry from the budget. Returns false when the budget is spent.
    pub fn try_acquire(&self) -> bool {
        if !self.is_enabled() {
            return true;
        }
        self.roll_window();

        let successes = self.successes.load(Ordering::Relaxed);
        let allowed = ((successes as f64) * self.ratio).floor() as u64;
        let allowed = allowed.max(self.min_per_window);

        let used = self.retries.fetch_add(1, Ordering::Relaxed);
        if used < allowed {
            true
        } else {
            // Give the token back so a later window is not starved by requests
            // that were already denied.
            self.retries.fetch_sub(1, Ordering::Relaxed);
            false
        }
    }
}

#[derive(Debug)]
pub struct UpstreamRuntime {
    pub addr: String,
    pub tls: bool,
    pub sni: String,
    pub weight: u16,
    pub verify_cert: bool,
    pub verify_hostname: bool,
    pub connect_timeout_ms: Option<u64>,
    pub total_connect_timeout_ms: Option<u64>,
    pub read_timeout_ms: Option<u64>,
    pub write_timeout_ms: Option<u64>,
    pub idle_timeout_ms: Option<u64>,
    state: Arc<UpstreamState>,
}

#[derive(Debug)]
struct UpstreamState {
    consecutive_failures: AtomicUsize,
    open_until_epoch_ms: AtomicU64,
    /// Active probe verdict. Starts healthy so that a reload never blackholes
    /// traffic while the first probes are still in flight.
    probe_healthy: AtomicBool,
    probe_successes: AtomicUsize,
    probe_failures: AtomicUsize,
    /// Epoch milliseconds of the last probe, so the checker knows what is due.
    last_probe_ms: AtomicU64,
    /// Requests currently in flight against this upstream.
    inflight: AtomicUsize,
    /// Moving average of observed latency, in microseconds. 0 means no sample
    /// has been recorded yet.
    ewma_us: AtomicU64,
}

impl Default for UpstreamState {
    /// Never derive this: `probe_healthy` has to start `true`, otherwise every
    /// upstream is considered down until its first probe lands, which would
    /// blackhole traffic on startup and after every config reload.
    fn default() -> Self {
        Self {
            consecutive_failures: AtomicUsize::new(0),
            open_until_epoch_ms: AtomicU64::new(0),
            probe_healthy: AtomicBool::new(true),
            probe_successes: AtomicUsize::new(0),
            probe_failures: AtomicUsize::new(0),
            last_probe_ms: AtomicU64::new(0),
            inflight: AtomicUsize::new(0),
            ewma_us: AtomicU64::new(0),
        }
    }
}

impl UpstreamRuntime {
    fn from_config(config: crate::config::UpstreamConfig) -> Self {
        let sni = config
            .sni
            .or_else(|| sni_from_addr(&config.addr))
            .unwrap_or_else(|| "localhost".to_string());
        Self {
            addr: config.addr,
            tls: config.tls,
            sni,
            weight: config.weight.max(1),
            verify_cert: config.verify_cert.unwrap_or(true),
            verify_hostname: config.verify_hostname.unwrap_or(true),
            connect_timeout_ms: config.connect_timeout_ms,
            total_connect_timeout_ms: config.total_connect_timeout_ms,
            read_timeout_ms: config.read_timeout_ms,
            write_timeout_ms: config.write_timeout_ms,
            idle_timeout_ms: config.idle_timeout_ms,
            state: Arc::new(UpstreamState::default()),
        }
    }

    /// Stable identifier for this upstream, used by sticky cookies so that a
    /// cookie keeps pointing at the same address across reloads, and stops
    /// matching when that address is gone.
    pub fn addr_hash(&self) -> u64 {
        hash_key(&[self.addr.as_str()])
    }

    pub fn is_circuit_open(&self) -> bool {
        !self.is_available_at(now_epoch_ms())
    }

    fn is_available_at(&self, now_ms: u64) -> bool {
        self.state.open_until_epoch_ms.load(Ordering::Relaxed) <= now_ms
            && self.state.probe_healthy.load(Ordering::Relaxed)
    }

    /// Requests currently in flight against this upstream.
    pub fn inflight(&self) -> usize {
        self.state.inflight.load(Ordering::Relaxed)
    }

    pub fn inc_inflight(&self) {
        self.state.inflight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_inflight(&self) {
        // saturating: a decrement without a matching increment must not wrap
        // the counter around to usize::MAX and take the upstream out of
        // consideration forever.
        let _ = self
            .state
            .inflight
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            });
    }

    /// Moving average latency in microseconds; 0 when nothing was measured yet.
    pub fn ewma_us(&self) -> u64 {
        self.state.ewma_us.load(Ordering::Relaxed)
    }

    /// Folds one latency sample into the moving average.
    pub fn record_latency(&self, sample_us: u64) {
        const ALPHA_NUM: u64 = 2;
        const ALPHA_DEN: u64 = 10;
        let _ = self
            .state
            .ewma_us
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(if current == 0 {
                    sample_us
                } else {
                    (current * (ALPHA_DEN - ALPHA_NUM) + sample_us * ALPHA_NUM) / ALPHA_DEN
                })
            });
    }

    /// Cost used by `p2c_ewma`: in-flight requests scaled by observed latency.
    /// `+1` keeps an idle upstream from looking free forever once it is slow.
    fn load_score(&self) -> u64 {
        let inflight = self.inflight() as u64 + 1;
        let latency = self.ewma_us().max(1);
        inflight.saturating_mul(latency)
    }

    /// Whether the active probe currently considers this upstream usable.
    pub fn is_probe_healthy(&self) -> bool {
        self.state.probe_healthy.load(Ordering::Relaxed)
    }

    /// Epoch milliseconds of the last probe, or 0 when it has never run.
    pub fn last_probe_ms(&self) -> u64 {
        self.state.last_probe_ms.load(Ordering::Relaxed)
    }

    /// Records a probe result and returns `Some(healthy)` when the verdict
    /// changed, so the caller can log and update metrics only on transitions.
    pub fn record_probe(
        &self,
        success: bool,
        healthy_threshold: u32,
        unhealthy_threshold: u32,
    ) -> Option<bool> {
        self.state
            .last_probe_ms
            .store(now_epoch_ms(), Ordering::Relaxed);

        if success {
            self.state.probe_failures.store(0, Ordering::Relaxed);
            let successes = self.state.probe_successes.fetch_add(1, Ordering::Relaxed) + 1;
            if !self.state.probe_healthy.load(Ordering::Relaxed)
                && successes >= healthy_threshold as usize
            {
                self.state.probe_healthy.store(true, Ordering::Relaxed);
                // An upstream that passed its probes is given a clean slate so
                // the passive breaker does not keep it out on old failures.
                self.state.consecutive_failures.store(0, Ordering::Relaxed);
                self.state.open_until_epoch_ms.store(0, Ordering::Relaxed);
                return Some(true);
            }
            return None;
        }

        self.state.probe_successes.store(0, Ordering::Relaxed);
        let failures = self.state.probe_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if self.state.probe_healthy.load(Ordering::Relaxed)
            && failures >= unhealthy_threshold as usize
        {
            self.state.probe_healthy.store(false, Ordering::Relaxed);
            return Some(false);
        }
        None
    }

    fn mark_failure(&self, circuit_breaker: &CircuitBreakerRuntime) -> bool {
        if !circuit_breaker.enabled {
            return false;
        }

        let failures = self
            .state
            .consecutive_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if failures < circuit_breaker.consecutive_failures {
            return false;
        }

        let now = now_epoch_ms();
        let was_open = self.state.open_until_epoch_ms.load(Ordering::Relaxed) > now;
        self.state.open_until_epoch_ms.store(
            now.saturating_add(circuit_breaker.open_ms),
            Ordering::Relaxed,
        );
        self.state.consecutive_failures.store(0, Ordering::Relaxed);
        !was_open
    }

    fn mark_success(&self) {
        self.state.consecutive_failures.store(0, Ordering::Relaxed);
        self.state.open_until_epoch_ms.store(0, Ordering::Relaxed);
    }
}

fn sni_from_addr(addr: &str) -> Option<String> {
    if addr.parse::<SocketAddr>().is_ok() {
        return None;
    }

    addr.split(':').next().map(ToString::to_string)
}

fn build_selection_ring(upstreams: &[UpstreamRuntime]) -> Vec<usize> {
    let mut ring = Vec::new();
    for (idx, upstream) in upstreams.iter().enumerate() {
        let weight = upstream_weight(upstream, idx);
        for _ in 0..weight {
            ring.push(idx);
        }
    }
    if ring.is_empty() {
        ring.extend(0..upstreams.len());
    }
    ring
}

fn upstream_weight(upstream: &UpstreamRuntime, _idx: usize) -> usize {
    upstream.weight.clamp(1, 256) as usize
}

/// Normalizes a Host header for matching: trimmed, lowercased, port removed.
///
/// Returns a borrow when the header is already in that shape, which is the
/// common case, so the request path does not allocate (T101). IPv6 literals in
/// brackets are kept as-is, port included, matching the previous behavior.
pub fn normalize_host(host: &str) -> Cow<'_, str> {
    let trimmed = host.trim();

    if trimmed.starts_with('[') {
        return if trimmed.bytes().any(|b| b.is_ascii_uppercase()) {
            Cow::Owned(trimmed.to_ascii_lowercase())
        } else {
            Cow::Borrowed(trimmed)
        };
    }

    let without_port = match trimmed.split_once(':') {
        Some((host, _)) => host,
        None => trimmed,
    };

    if without_port.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(without_port.to_ascii_lowercase())
    } else {
        Cow::Borrowed(without_port)
    }
}

/// Normalizes a configured host pattern, keeping a leading `*.` intact.
fn normalize_host_pattern(pattern: &str) -> Cow<'_, str> {
    match pattern.strip_prefix("*.") {
        Some(suffix) => match normalize_host(suffix) {
            Cow::Borrowed(suffix) if suffix.len() == pattern.len() - 2 => Cow::Borrowed(pattern),
            other => Cow::Owned(format!("*.{other}")),
        },
        None => normalize_host(pattern),
    }
}

pub fn hash_key(parts: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    hasher.finish()
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CircuitBreakerConfig, ObservabilityConfig, RouteConfig, ServerConfig, ServiceConfig,
        UpstreamConfig,
    };

    fn upstream(addr: &str) -> UpstreamConfig {
        UpstreamConfig {
            addr: addr.to_string(),
            tls: false,
            sni: None,
            weight: 1,
            verify_cert: None,
            verify_hostname: None,
            connect_timeout_ms: None,
            total_connect_timeout_ms: None,
            read_timeout_ms: None,
            write_timeout_ms: None,
            idle_timeout_ms: None,
        }
    }

    fn service(
        name: &str,
        lb: LbStrategy,
        max_retries: usize,
        upstreams: Vec<UpstreamConfig>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            lb,
            upstream_h2: UpstreamH2::default(),
            max_retries,
            retry_backoff_ms: 0,
            circuit_breaker: no_breaker(),
            upstreams,
            ..Default::default()
        }
    }

    fn route(
        name: &str,
        service: &str,
        host: Option<&str>,
        path_prefix: &str,
        is_default: bool,
    ) -> RouteConfig {
        RouteConfig {
            name: name.to_string(),
            service: service.to_string(),
            host: host.map(ToString::to_string),
            path_prefix: path_prefix.to_string(),
            methods: Vec::new(),
            is_default,
            ..Default::default()
        }
    }

    fn no_breaker() -> CircuitBreakerConfig {
        CircuitBreakerConfig::default()
    }

    fn runtime_from_parts(services: Vec<ServiceConfig>, routes: Vec<RouteConfig>) -> RuntimeConfig {
        RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
            compression: Default::default(),
            services,
            routes,
        })
    }

    #[test]
    fn select_route_returns_none_when_no_route_matches_and_no_default() {
        let runtime = runtime_from_parts(
            vec![service(
                "api",
                LbStrategy::RoundRobin,
                0,
                vec![upstream("127.0.0.1:9000")],
            )],
            vec![route("api", "api", Some("api.local"), "/api", false)],
        );

        assert_eq!(runtime.select_route("www.local", "/"), None);
    }

    #[test]
    fn select_route_uses_default_route_when_present() {
        let runtime = runtime_from_parts(
            vec![
                service(
                    "api",
                    LbStrategy::RoundRobin,
                    0,
                    vec![upstream("127.0.0.1:9000")],
                ),
                service(
                    "default",
                    LbStrategy::RoundRobin,
                    0,
                    vec![upstream("127.0.0.1:9001")],
                ),
            ],
            vec![
                route("api", "api", Some("api.local"), "/api", false),
                route("default", "default", None, "/", true),
            ],
        );

        let idx = runtime
            .select_route("no-match.local", "/anything")
            .expect("default route should match");
        assert_eq!(runtime.route(idx).map(|r| r.name.as_ref()), Some("default"));
    }

    #[test]
    fn next_upstream_skips_attempted_candidate_for_failover() {
        let runtime = runtime_from_parts(
            vec![service(
                "default",
                LbStrategy::Hash,
                1,
                vec![upstream("127.0.0.1:9100"), upstream("127.0.0.1:9101")],
            )],
            vec![route("default", "default", None, "/", true)],
        );

        let route_idx = runtime
            .select_route("example.local", "/")
            .expect("route selected");
        let route = runtime.route(route_idx).expect("route exists");
        let svc = runtime.service(route.service_idx).expect("service exists");

        let (first_idx, _) = svc.next_upstream(0, &[]).expect("initial upstream");
        let (second_idx, _) = svc
            .next_upstream(0, &[first_idx])
            .expect("failover upstream");

        assert_ne!(first_idx, second_idx);
    }

    #[test]
    fn normalize_host_lowercases_and_strips_port() {
        assert_eq!(normalize_host("Example.COM:8443"), "example.com");
    }

    #[test]
    fn circuit_breaker_opens_after_failure_threshold() {
        let breaker = CircuitBreakerConfig {
            enabled: true,
            consecutive_failures: 1,
            open_ms: 60_000,
        };
        let svc = ServiceConfig {
            name: "default".to_string(),
            lb: LbStrategy::RoundRobin,
            upstream_h2: UpstreamH2::default(),
            max_retries: 1,
            retry_backoff_ms: 0,
            circuit_breaker: breaker,
            upstreams: vec![upstream("127.0.0.1:9200"), upstream("127.0.0.1:9201")],
            ..Default::default()
        };
        let runtime = runtime_from_parts(
            vec![svc],
            vec![route("default", "default", None, "/", true)],
        );

        let route = runtime.route(0).expect("route exists");
        let service = runtime.service(route.service_idx).expect("service exists");

        let opened = service.mark_upstream_failure(0);
        assert!(opened);
        assert!(service.upstreams[0].is_circuit_open());

        let (next_idx, _) = service.next_upstream(0, &[]).expect("next upstream");
        assert_eq!(next_idx, 1);
    }

    #[test]
    fn readiness_fails_when_all_upstreams_are_open_circuit() {
        let breaker = CircuitBreakerConfig {
            enabled: true,
            consecutive_failures: 1,
            open_ms: 60_000,
        };
        let svc = ServiceConfig {
            name: "default".to_string(),
            lb: LbStrategy::RoundRobin,
            upstream_h2: UpstreamH2::default(),
            max_retries: 1,
            retry_backoff_ms: 0,
            circuit_breaker: breaker,
            upstreams: vec![upstream("127.0.0.1:9300")],
            ..Default::default()
        };
        let runtime = runtime_from_parts(
            vec![svc],
            vec![route("default", "default", None, "/", true)],
        );

        let route = runtime.route(0).expect("route exists");
        let service = runtime.service(route.service_idx).expect("service exists");
        assert!(runtime.is_ready());
        service.mark_upstream_failure(0);
        assert!(!runtime.is_ready());
    }

    #[test]
    fn route_resolves_correct_service_index() {
        let runtime = runtime_from_parts(
            vec![
                service(
                    "first",
                    LbStrategy::RoundRobin,
                    0,
                    vec![upstream("127.0.0.1:8001")],
                ),
                service(
                    "second",
                    LbStrategy::RoundRobin,
                    0,
                    vec![upstream("127.0.0.1:8002")],
                ),
            ],
            vec![
                route("r1", "second", None, "/a", false),
                route("r2", "first", None, "/b", false),
            ],
        );

        let r1 = runtime.route(0).expect("r1 exists");
        assert_eq!(r1.service_idx, 1); // "second" is at index 1
        assert_eq!(runtime.service(r1.service_idx).unwrap().name, "second");

        let r2 = runtime.route(1).expect("r2 exists");
        assert_eq!(r2.service_idx, 0); // "first" is at index 0
        assert_eq!(runtime.service(r2.service_idx).unwrap().name, "first");
    }
}

#[cfg(test)]
mod header_rule_tests {
    use super::*;

    #[test]
    fn route_header_rules_are_compiled_from_toml() {
        let toml = r#"
[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:3001"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true

[route.request_headers]
set = { "X-Real-IP" = "$client_ip" }
"#;
        let config = PrxConfig::from_toml_str(toml).expect("config should be valid");
        assert!(
            !config.routes[0].request_headers.set.is_empty(),
            "TOML did not reach RouteConfig"
        );

        let runtime = RuntimeConfig::from_config(config);
        let route = runtime.route(0).expect("route 0 exists");
        assert!(
            !route.request_headers.is_empty(),
            "route header rules were not compiled into the runtime"
        );
        assert!(route.request_headers.needs_client_addr());
    }
}

#[cfg(test)]
mod lb_tests {
    use super::*;
    use crate::config::{ServiceConfig, UpstreamConfig};

    fn upstream_with(addr: &str, weight: u16) -> UpstreamConfig {
        UpstreamConfig {
            addr: addr.to_string(),
            tls: false,
            sni: None,
            weight,
            verify_cert: None,
            verify_hostname: None,
            connect_timeout_ms: None,
            total_connect_timeout_ms: None,
            read_timeout_ms: None,
            write_timeout_ms: None,
            idle_timeout_ms: None,
        }
    }

    fn service_with(lb: LbStrategy, upstreams: Vec<UpstreamConfig>) -> ServiceRuntime {
        ServiceRuntime::from_config(ServiceConfig {
            name: "svc".to_string(),
            lb,
            upstreams,
            ..Default::default()
        })
    }

    /// T114: weight is honored, which was worth proving rather than assuming -
    /// the helper that reads it had an unused parameter that made it look dead.
    #[test]
    fn weight_shapes_the_round_robin_distribution() {
        let service = service_with(
            LbStrategy::RoundRobin,
            vec![
                upstream_with("127.0.0.1:9000", 3),
                upstream_with("127.0.0.1:9001", 1),
            ],
        );

        let mut counts = [0usize; 2];
        for _ in 0..4_000 {
            let (idx, _) = service
                .next_upstream(0, &[])
                .expect("an upstream is available");
            counts[idx] += 1;
        }

        let ratio = counts[0] as f64 / counts[1] as f64;
        assert!(
            (2.85..=3.15).contains(&ratio),
            "expected roughly 3:1, got {ratio:.2} ({counts:?})"
        );
    }

    /// T114: least_conn must prefer the upstream with fewer requests in flight.
    #[test]
    fn least_conn_avoids_the_busy_upstream() {
        let service = service_with(
            LbStrategy::LeastConn,
            vec![
                upstream_with("127.0.0.1:9000", 1),
                upstream_with("127.0.0.1:9001", 1),
            ],
        );

        // Upstream 0 is saturated.
        for _ in 0..50 {
            service.upstreams[0].inc_inflight();
        }

        let mut counts = [0usize; 2];
        for _ in 0..1_000 {
            let (idx, _) = service
                .next_upstream(0, &[])
                .expect("an upstream is available");
            counts[idx] += 1;
        }

        assert!(
            counts[1] > counts[0] * 10,
            "the idle upstream should take nearly all of the traffic, got {counts:?}"
        );
    }

    /// T114: p2c_ewma must also avoid an upstream that accepts requests but is
    /// slow, which least_conn alone cannot see.
    #[test]
    fn p2c_ewma_avoids_the_slow_upstream() {
        let service = service_with(
            LbStrategy::P2cEwma,
            vec![
                upstream_with("127.0.0.1:9000", 1),
                upstream_with("127.0.0.1:9001", 1),
            ],
        );

        // Same in-flight count, very different latency.
        for _ in 0..20 {
            service.upstreams[0].record_latency(200_000);
            service.upstreams[1].record_latency(2_000);
        }

        let mut counts = [0usize; 2];
        for _ in 0..1_000 {
            let (idx, _) = service
                .next_upstream(0, &[])
                .expect("an upstream is available");
            counts[idx] += 1;
        }

        assert!(
            counts[1] > counts[0] * 10,
            "the fast upstream should take nearly all of the traffic, got {counts:?}"
        );
    }

    /// T114: the moving average must react to new samples without being thrown
    /// off by a single outlier.
    #[test]
    fn latency_average_is_smoothed() {
        let service = service_with(
            LbStrategy::P2cEwma,
            vec![upstream_with("127.0.0.1:9000", 1)],
        );
        let upstream = &service.upstreams[0];

        assert_eq!(upstream.ewma_us(), 0, "no sample means no average yet");
        upstream.record_latency(1_000);
        assert_eq!(
            upstream.ewma_us(),
            1_000,
            "the first sample sets the average"
        );

        upstream.record_latency(100_000);
        let after_spike = upstream.ewma_us();
        assert!(
            after_spike > 1_000 && after_spike < 50_000,
            "one spike must move the average without dominating it, got {after_spike}"
        );
    }

    /// T114: in-flight counting must be balanced, and must never wrap.
    #[test]
    fn inflight_counting_is_balanced_and_saturating() {
        let service = service_with(
            LbStrategy::LeastConn,
            vec![upstream_with("127.0.0.1:9000", 1)],
        );
        let upstream = &service.upstreams[0];

        upstream.inc_inflight();
        upstream.inc_inflight();
        assert_eq!(upstream.inflight(), 2);

        upstream.dec_inflight();
        upstream.dec_inflight();
        assert_eq!(upstream.inflight(), 0);

        // An extra decrement must not wrap around and hide the upstream.
        upstream.dec_inflight();
        assert_eq!(upstream.inflight(), 0);
    }

    /// T114: a sticky key points at one upstream, and stops pointing anywhere
    /// once that upstream is unavailable, so affinity never costs availability.
    #[test]
    fn sticky_selection_falls_back_when_the_pinned_upstream_is_down() {
        let service = service_with(
            LbStrategy::RoundRobin,
            vec![
                upstream_with("127.0.0.1:9000", 1),
                upstream_with("127.0.0.1:9001", 1),
            ],
        );

        let pinned_hash = service.upstreams[1].addr_hash();
        assert_eq!(service.select_sticky(pinned_hash, &[]), Some(1));

        // An address that is not in this service pins nothing.
        assert_eq!(
            service.select_sticky(hash_key(&["127.0.0.1:9999"]), &[]),
            None
        );

        // Marking it unhealthy releases the pin.
        service.upstreams[1].record_probe(false, 1, 1);
        assert_eq!(service.select_sticky(pinned_hash, &[]), None);
    }
}
