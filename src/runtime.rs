use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rand::Rng;

use crate::config::{LbStrategy, PrxConfig};

#[derive(Debug)]
pub struct RuntimeConfig {
    routes: Vec<RouteRuntime>,
}

impl RuntimeConfig {
    pub fn from_config(config: PrxConfig) -> Self {
        let mut routes = config
            .routes
            .into_iter()
            .map(RouteRuntime::from_config)
            .collect::<Vec<_>>();
        routes.sort_by(|a, b| {
            b.path_prefix
                .len()
                .cmp(&a.path_prefix.len())
                .then_with(|| a.name.cmp(&b.name))
        });
        Self { routes }
    }

    pub fn select_route(&self, host: &str, path: &str) -> Option<usize> {
        let normalized = normalize_host(host);
        let mut fallback_idx = None;

        for (idx, route) in self.routes.iter().enumerate() {
            if route.is_default && fallback_idx.is_none() {
                fallback_idx = Some(idx);
            }

            if !route.matches_host(&normalized) {
                continue;
            }

            if path.starts_with(&route.path_prefix) {
                return Some(idx);
            }
        }

        fallback_idx
    }

    pub fn route(&self, idx: usize) -> Option<&RouteRuntime> {
        self.routes.get(idx)
    }

    pub fn is_ready(&self) -> bool {
        self.routes.iter().all(RouteRuntime::has_available_upstream)
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
    pub name: String,
    pub host: Option<String>,
    pub path_prefix: String,
    pub is_default: bool,
    pub lb: LbStrategy,
    pub max_retries: usize,
    pub retry_backoff_ms: u64,
    pub circuit_breaker: CircuitBreakerRuntime,
    pub upstreams: Vec<UpstreamRuntime>,
    ring: Vec<usize>,
    rr_cursor: Arc<AtomicUsize>,
}

impl RouteRuntime {
    fn from_config(config: crate::config::RouteConfig) -> Self {
        let host = config.host.as_deref().map(normalize_host);
        let circuit_breaker = CircuitBreakerRuntime::from_config(&config.circuit_breaker);
        let upstreams = config
            .upstreams
            .into_iter()
            .map(UpstreamRuntime::from_config)
            .collect::<Vec<_>>();
        let ring = build_selection_ring(&upstreams);

        Self {
            name: config.name,
            host,
            path_prefix: config.path_prefix,
            is_default: config.is_default,
            lb: config.lb,
            max_retries: config.max_retries,
            retry_backoff_ms: config.retry_backoff_ms,
            circuit_breaker,
            upstreams,
            ring,
            rr_cursor: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn matches_host(&self, request_host: &str) -> bool {
        let Some(pattern) = &self.host else {
            return true;
        };

        if let Some(suffix) = pattern.strip_prefix("*.") {
            request_host == suffix || request_host.ends_with(&format!(".{suffix}"))
        } else {
            pattern == request_host
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

#[derive(Debug, Default)]
struct UpstreamState {
    consecutive_failures: AtomicUsize,
    open_until_epoch_ms: AtomicU64,
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

    pub fn is_circuit_open(&self) -> bool {
        !self.is_available_at(now_epoch_ms())
    }

    fn is_available_at(&self, now_ms: u64) -> bool {
        self.state.open_until_epoch_ms.load(Ordering::Relaxed) <= now_ms
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

pub fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().to_ascii_lowercase();
    if trimmed.starts_with('[') {
        return trimmed;
    }

    trimmed
        .split_once(':')
        .map(|(h, _)| h.to_string())
        .unwrap_or(trimmed)
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
        CircuitBreakerConfig, ObservabilityConfig, RouteConfig, ServerConfig, UpstreamConfig,
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

    fn route(
        name: &str,
        host: Option<&str>,
        path_prefix: &str,
        is_default: bool,
        upstreams: Vec<UpstreamConfig>,
    ) -> RouteConfig {
        RouteConfig {
            name: name.to_string(),
            host: host.map(ToString::to_string),
            path_prefix: path_prefix.to_string(),
            is_default,
            lb: LbStrategy::RoundRobin,
            max_retries: 0,
            retry_backoff_ms: 0,
            circuit_breaker: no_breaker(),
            upstreams,
        }
    }

    fn no_breaker() -> CircuitBreakerConfig {
        CircuitBreakerConfig::default()
    }

    fn runtime_from_routes(routes: Vec<RouteConfig>) -> RuntimeConfig {
        RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            routes,
        })
    }

    #[test]
    fn select_route_returns_none_when_no_route_matches_and_no_default() {
        let runtime = runtime_from_routes(vec![route(
            "api",
            Some("api.local"),
            "/api",
            false,
            vec![upstream("127.0.0.1:9000")],
        )]);

        assert_eq!(runtime.select_route("www.local", "/"), None);
    }

    #[test]
    fn select_route_uses_default_route_when_present() {
        let runtime = runtime_from_routes(vec![
            route(
                "api",
                Some("api.local"),
                "/api",
                false,
                vec![upstream("127.0.0.1:9000")],
            ),
            route("default", None, "/", true, vec![upstream("127.0.0.1:9001")]),
        ]);

        let idx = runtime
            .select_route("no-match.local", "/anything")
            .expect("default route should match");
        assert_eq!(runtime.route(idx).map(|r| r.name.as_str()), Some("default"));
    }

    #[test]
    fn next_upstream_skips_attempted_candidate_for_failover() {
        let mut hash_route = route(
            "default",
            None,
            "/",
            true,
            vec![upstream("127.0.0.1:9100"), upstream("127.0.0.1:9101")],
        );
        hash_route.lb = LbStrategy::Hash;
        hash_route.max_retries = 1;
        let runtime = runtime_from_routes(vec![hash_route]);

        let idx = runtime
            .select_route("example.local", "/")
            .expect("route selected");
        let route = runtime.route(idx).expect("route exists");

        let (first_idx, _) = route.next_upstream(0, &[]).expect("initial upstream");
        let (second_idx, _) = route
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
        let mut cb_route = route(
            "default",
            None,
            "/",
            true,
            vec![upstream("127.0.0.1:9200"), upstream("127.0.0.1:9201")],
        );
        cb_route.max_retries = 1;
        cb_route.circuit_breaker = breaker;
        let runtime = runtime_from_routes(vec![cb_route]);

        let route_idx = runtime
            .select_route("example.local", "/")
            .expect("route selected");
        let route = runtime.route(route_idx).expect("route exists");

        let opened = route.mark_upstream_failure(0);
        assert!(opened);
        assert!(route.upstreams[0].is_circuit_open());

        let (next_idx, _) = route.next_upstream(0, &[]).expect("next upstream");
        assert_eq!(next_idx, 1);
    }

    #[test]
    fn readiness_fails_when_all_upstreams_are_open_circuit() {
        let breaker = CircuitBreakerConfig {
            enabled: true,
            consecutive_failures: 1,
            open_ms: 60_000,
        };
        let mut cb_route = route("default", None, "/", true, vec![upstream("127.0.0.1:9300")]);
        cb_route.max_retries = 1;
        cb_route.circuit_breaker = breaker;
        let runtime = runtime_from_routes(vec![cb_route]);

        let route = runtime.route(0).expect("route exists");
        assert!(runtime.is_ready());
        route.mark_upstream_failure(0);
        assert!(!runtime.is_ready());
    }
}
