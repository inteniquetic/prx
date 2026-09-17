use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use bytes::Bytes;
use pingora::prelude::*;
use pingora::upstreams::peer::ALPN;
use tracing::{debug, error, info, warn};

use crate::config::{LbStrategy, UpstreamH2};
use crate::metrics;
use crate::router::RouteMatch;
use crate::runtime::{RuntimeConfig, hash_key, normalize_host};

/// Names used for requests that never reach a configured route. Pre-built so
/// the request path never allocates one (T101).
struct StaticNames {
    health: Arc<str>,
    ready: Arc<str>,
    no_route: Arc<str>,
    method_not_allowed: Arc<str>,
    unknown: Arc<str>,
}

impl Default for StaticNames {
    fn default() -> Self {
        Self {
            health: Arc::from("health"),
            ready: Arc::from("ready"),
            no_route: Arc::from("no_route"),
            method_not_allowed: Arc::from("method_not_allowed"),
            unknown: Arc::from("unknown"),
        }
    }
}

pub struct PrxProxy {
    active_config: Arc<ArcSwap<RuntimeConfig>>,
    access_log: bool,
    health_path: String,
    ready_path: String,
    names: StaticNames,
}

impl PrxProxy {
    pub fn new(
        active_config: Arc<ArcSwap<RuntimeConfig>>,
        access_log: bool,
        health_path: String,
        ready_path: String,
    ) -> Self {
        Self {
            active_config,
            access_log,
            health_path,
            ready_path,
            names: StaticNames::default(),
        }
    }

    fn should_retry(&self, ctx: &mut RequestCtx) -> bool {
        let Some(snapshot) = &ctx.snapshot else {
            return false;
        };
        let Some(route_idx) = ctx.route_idx else {
            return false;
        };
        let Some(route) = snapshot.route(route_idx) else {
            return false;
        };
        let Some(service) = snapshot.service(route.service_idx) else {
            return false;
        };

        if ctx.retries >= service.max_retries {
            return false;
        }
        if ctx.attempted_upstreams.len() >= service.upstreams.len() {
            return false;
        }

        ctx.retries += 1;
        true
    }

    async fn respond_text(session: &mut Session, status: u16, body: &'static str) -> Result<bool> {
        session
            .respond_error_with_body(status, Bytes::from_static(body.as_bytes()))
            .await?;
        Ok(true)
    }

    fn record_upstream_failure(&self, ctx: &mut RequestCtx, stage: &'static str) {
        let Some(snapshot) = &ctx.snapshot else {
            return;
        };
        let Some(route_idx) = ctx.route_idx else {
            return;
        };
        let Some(route) = snapshot.route(route_idx) else {
            return;
        };
        let Some(service) = snapshot.service(route.service_idx) else {
            return;
        };
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return;
        };
        let Some(upstream) = service.upstreams.get(upstream_idx) else {
            return;
        };

        metrics::inc_upstream_error(route.name.as_ref(), upstream.addr.as_str(), stage);
        let opened = service.mark_upstream_failure(upstream_idx);
        let is_open = upstream.is_circuit_open();
        metrics::set_circuit_state(route.name.as_ref(), upstream.addr.as_str(), is_open);
        if opened {
            metrics::mark_circuit_open(route.name.as_ref(), upstream.addr.as_str());
            warn!(
                route = route.name.as_ref(),
                service = service.name.as_str(),
                upstream = upstream.addr.as_str(),
                "opened circuit breaker for upstream"
            );
        }
    }

    fn record_upstream_success(&self, ctx: &mut RequestCtx) {
        let Some(snapshot) = &ctx.snapshot else {
            return;
        };
        let Some(route_idx) = ctx.route_idx else {
            return;
        };
        let Some(route) = snapshot.route(route_idx) else {
            return;
        };
        let Some(service) = snapshot.service(route.service_idx) else {
            return;
        };
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return;
        };
        let Some(upstream) = service.upstreams.get(upstream_idx) else {
            return;
        };

        service.mark_upstream_success(upstream_idx);
        metrics::set_circuit_state(route.name.as_ref(), upstream.addr.as_str(), false);
    }
}

/// What `request_filter` decided while the request header was still borrowed.
#[derive(Debug, Clone, Copy)]
enum Decision {
    Health,
    Ready,
    Route(RouteMatch),
}

pub struct RequestCtx {
    started_at: Instant,
    snapshot: Option<Arc<RuntimeConfig>>,
    route_idx: Option<usize>,
    service_idx: Option<usize>,
    attempted_upstreams: Vec<usize>,
    retries: usize,
    /// Refcount bump instead of a `String` clone per request (T101).
    route_name: Option<Arc<str>>,
    /// The client asked to upgrade the connection (websocket and friends).
    is_upgrade: bool,
    upstream_addr: Option<String>,
}

impl Default for RequestCtx {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            snapshot: None,
            route_idx: None,
            service_idx: None,
            attempted_upstreams: Vec::new(),
            retries: 0,
            route_name: None,
            is_upgrade: false,
            upstream_addr: None,
        }
    }
}

#[async_trait]
impl ProxyHttp for PrxProxy {
    type CTX = RequestCtx;

    fn new_ctx(&self) -> Self::CTX {
        Self::CTX::default()
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        // `load()` hands out a guard without touching the Arc refcount, which
        // matters because every worker thread reads this same cacheline on
        // every request. The refcount is only paid for requests that go on to
        // an upstream: health checks, 404s and 405s never clone it (T103).
        let snapshot = self.active_config.load();

        // Everything that borrows the request header is resolved here, so the
        // borrow ends before the session is used mutably below. Nothing in this
        // block allocates: the host is borrowed from the header when it is
        // already lowercase and port-free, and the match result is Copy (T101).
        let decision = {
            let req_header = session.req_header();
            let path = req_header.uri.path();
            ctx.is_upgrade = req_header.headers.contains_key("upgrade");

            if path == self.health_path {
                Decision::Health
            } else if path == self.ready_path {
                Decision::Ready
            } else {
                let host = req_header
                    .headers
                    .get("host")
                    .and_then(|val| val.to_str().ok())
                    .unwrap_or("localhost");
                let host = normalize_host(host);
                Decision::Route(snapshot.select(&host, path, Some(req_header.method.as_str())))
            }
        };

        match decision {
            Decision::Health => {
                ctx.route_name = Some(self.names.health.clone());
                Self::respond_text(session, 200, "ok\n").await
            }
            Decision::Ready => {
                ctx.route_name = Some(self.names.ready.clone());
                if snapshot.is_ready() {
                    Self::respond_text(session, 200, "ready\n").await
                } else {
                    Self::respond_text(session, 503, "not_ready\n").await
                }
            }
            Decision::Route(RouteMatch::Matched(route_idx)) => {
                // One snapshot for the whole request: every later phase reads
                // this Arc rather than loading again, so a reload mid-request
                // can never pair a route from one config with a service from
                // another (T103).
                ctx.snapshot = Some(Arc::clone(&snapshot));
                ctx.route_idx = Some(route_idx);
                if let Some(route) = snapshot.route(route_idx) {
                    ctx.service_idx = Some(route.service_idx);
                    ctx.route_name = Some(route.name.clone());
                    debug!(route = %route.name, "matched route");
                }
                Ok(false)
            }
            Decision::Route(RouteMatch::MethodNotAllowed) => {
                ctx.route_name = Some(self.names.method_not_allowed.clone());
                warn!(
                    "{}: method not allowed for the matching route",
                    session.request_summary()
                );
                Self::respond_text(session, 405, "method_not_allowed\n").await
            }
            Decision::Route(RouteMatch::NotFound) => {
                ctx.route_name = Some(self.names.no_route.clone());
                warn!("{}: no route matched", session.request_summary());
                session.respond_error(404).await?;
                Ok(true)
            }
        }
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // request_filter stored the snapshot this request is pinned to. Loading
        // a fresh one here would risk pairing a route with a service from a
        // different config version, so this only happens if the request somehow
        // reached the upstream phase without going through request_filter.
        let snapshot = match &ctx.snapshot {
            Some(snapshot) => Arc::clone(snapshot),
            None => {
                let snapshot = self.active_config.load_full();
                ctx.snapshot = Some(Arc::clone(&snapshot));
                snapshot
            }
        };

        let route_idx = match ctx.route_idx {
            Some(idx) => idx,
            None => {
                return Error::e_explain(
                    HTTPStatus(404),
                    format!("no route matched for {}", session.request_summary()),
                );
            }
        };
        ctx.route_idx = Some(route_idx);

        let Some(route) = snapshot.route(route_idx) else {
            return Error::e_explain(
                InternalError,
                format!("selected route index is out of bounds: {route_idx}"),
            );
        };

        let Some(service) = snapshot.service(route.service_idx) else {
            return Error::e_explain(
                InternalError,
                format!(
                    "route '{}' references service index {} which is out of bounds",
                    route.name, route.service_idx
                ),
            );
        };

        if ctx.retries > 0 && service.retry_backoff_ms > 0 {
            tokio::time::sleep(Duration::from_millis(service.retry_backoff_ms)).await;
        }

        // Only the hash strategy needs a key, so the other strategies do not
        // pay for hashing the host and path on every request (T101).
        let hash_seed = if matches!(service.lb, LbStrategy::Hash) {
            let req_header = session.req_header();
            let host = req_header
                .headers
                .get("host")
                .and_then(|val| val.to_str().ok())
                .unwrap_or("");
            hash_key(&[host, req_header.uri.path()])
        } else {
            0
        };
        let (upstream_idx, upstream) =
            if let Some(selected) = service.next_upstream(hash_seed, &ctx.attempted_upstreams) {
                selected
            } else {
                ctx.attempted_upstreams.clear();
                if let Some(selected) = service.next_upstream(hash_seed, &ctx.attempted_upstreams) {
                    selected
                } else {
                    return Error::e_explain(
                        InternalError,
                        format!(
                            "service '{}' (via route '{}') has no selectable upstreams",
                            service.name, route.name
                        ),
                    );
                }
            };
        ctx.attempted_upstreams.push(upstream_idx);
        ctx.upstream_addr = Some(upstream.addr.clone());

        let mut peer = HttpPeer::new(upstream.addr.clone(), upstream.tls, upstream.sni.clone());
        // Without this the peer defaults to HTTP/1.1, which makes gRPC
        // impossible: it needs end-to-end HTTP/2 to carry trailers (T115).
        peer.options.alpn = match service.upstream_h2 {
            UpstreamH2::Never => ALPN::H1,
            UpstreamH2::Always => ALPN::H2,
            UpstreamH2::Auto => ALPN::H2H1,
        };
        peer.options.verify_cert = upstream.verify_cert;
        peer.options.verify_hostname = upstream.verify_hostname;
        if let Some(ms) = upstream.connect_timeout_ms {
            peer.options.connection_timeout = Some(Duration::from_millis(ms));
        }
        if let Some(ms) = upstream.total_connect_timeout_ms {
            peer.options.total_connection_timeout = Some(Duration::from_millis(ms));
        }
        // read/write timeouts describe request/response traffic. An upgraded
        // connection (websocket, and anything else that takes over the socket)
        // is expected to sit idle for long stretches, so applying them there
        // would kill healthy connections (T115).
        if !ctx.is_upgrade {
            if let Some(ms) = upstream.read_timeout_ms {
                peer.options.read_timeout = Some(Duration::from_millis(ms));
            }
            if let Some(ms) = upstream.write_timeout_ms {
                peer.options.write_timeout = Some(Duration::from_millis(ms));
            }
            if let Some(ms) = upstream.idle_timeout_ms {
                peer.options.idle_timeout = Some(Duration::from_millis(ms));
            }
        }

        Ok(Box::new(peer))
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let Some(snapshot) = &ctx.snapshot else {
            return Ok(());
        };
        let Some(route_idx) = ctx.route_idx else {
            return Ok(());
        };
        let Some(route) = snapshot.route(route_idx) else {
            return Ok(());
        };
        let Some(service) = snapshot.service(route.service_idx) else {
            return Ok(());
        };
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return Ok(());
        };
        let Some(upstream) = service.upstreams.get(upstream_idx) else {
            return Ok(());
        };

        // Keep Host aligned with SNI when proxying to strict virtual hosts.
        upstream_request.insert_header("host", upstream.sni.as_str())?;
        self.record_upstream_success(ctx);
        Ok(())
    }

    fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<Error>,
    ) -> Box<Error> {
        self.record_upstream_failure(ctx, "connect");
        e.set_retry(self.should_retry(ctx));
        e
    }

    fn error_while_proxy(
        &self,
        _peer: &HttpPeer,
        _session: &mut Session,
        mut e: Box<Error>,
        ctx: &mut Self::CTX,
        _client_reused: bool,
    ) -> Box<Error> {
        warn!(
            upstream = ctx.upstream_addr.as_deref().unwrap_or("-"),
            error = %e,
            "proxying error"
        );
        self.record_upstream_failure(ctx, "proxy");
        e.set_retry(self.should_retry(ctx));
        e
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX) {
        if !self.access_log {
            return;
        }

        let latency_ms = ctx.started_at.elapsed().as_millis();
        let summary = session.request_summary();
        let route_name = ctx.route_name.clone().unwrap_or_else(|| {
            ctx.snapshot
                .as_ref()
                .and_then(|cfg| ctx.route_idx.and_then(|idx| cfg.route(idx)))
                .map(|route| route.name.clone())
                .unwrap_or_else(|| self.names.unknown.clone())
        });
        let status = session
            .response_written()
            .map(|resp| resp.status.as_u16())
            .unwrap_or_else(|| if e.is_some() { 500 } else { 0 });
        metrics::observe_request(route_name.as_ref(), status, latency_ms as f64);

        if let Some(err) = e {
            error!(
                route = route_name.as_ref(),
                upstream = ctx.upstream_addr.as_deref().unwrap_or("-"),
                retries = ctx.retries,
                latency_ms,
                error = %err,
                "{}",
                summary
            );
            return;
        }

        info!(
            route = route_name.as_ref(),
            upstream = ctx.upstream_addr.as_deref().unwrap_or("-"),
            retries = ctx.retries,
            latency_ms,
            "{}",
            summary
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CircuitBreakerConfig, LbStrategy, ObservabilityConfig, PrxConfig, RouteConfig,
        ServerConfig, ServiceConfig, UpstreamConfig,
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

    fn service(name: &str, max_retries: usize, upstream_count: usize) -> ServiceConfig {
        let upstreams = (0..upstream_count)
            .map(|idx| upstream(&format!("127.0.0.1:{}", 9000 + idx)))
            .collect::<Vec<_>>();

        ServiceConfig {
            name: name.to_string(),
            lb: LbStrategy::RoundRobin,
            upstream_h2: Default::default(),
            max_retries,
            retry_backoff_ms: 0,
            circuit_breaker: CircuitBreakerConfig::default(),
            upstreams,
        }
    }

    fn route(name: &str, service: &str) -> RouteConfig {
        RouteConfig {
            name: name.to_string(),
            service: service.to_string(),
            host: None,
            path_prefix: "/".to_string(),
            methods: Vec::new(),
            is_default: true,
        }
    }

    fn build_runtime(max_retries: usize, upstream_count: usize) -> Arc<RuntimeConfig> {
        Arc::new(RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            services: vec![service("default", max_retries, upstream_count)],
            routes: vec![route("default", "default")],
        }))
    }

    fn build_proxy(runtime: Arc<RuntimeConfig>) -> PrxProxy {
        PrxProxy::new(
            Arc::new(ArcSwap::new(runtime)),
            false,
            "/healthz".to_string(),
            "/readyz".to_string(),
        )
    }

    /// T103: a request is pinned to the snapshot it started with, so a reload
    /// mid-request cannot pair a route from one config with a service from the
    /// next one.
    #[test]
    fn request_keeps_its_snapshot_across_a_reload() {
        let first = build_runtime(0, 2);
        let active = Arc::new(ArcSwap::new(first.clone()));

        // A request that has already matched a route holds its own snapshot.
        let ctx = RequestCtx {
            snapshot: Some(Arc::clone(&active.load())),
            route_idx: Some(0),
            service_idx: Some(0),
            ..RequestCtx::default()
        };

        // Config is replaced while that request is still in flight.
        let second = build_runtime(0, 1);
        active.store(second.clone());

        let pinned = ctx.snapshot.as_ref().expect("snapshot is pinned");
        assert!(
            Arc::ptr_eq(pinned, &first),
            "the in-flight request must keep the snapshot it started with"
        );
        let route = pinned.route(0).expect("route 0 exists in the old snapshot");
        let service = pinned
            .service(route.service_idx)
            .expect("service still resolves inside the pinned snapshot");
        assert_eq!(
            service.upstreams.len(),
            2,
            "the pinned snapshot must still describe the old upstream set"
        );

        // New requests do see the new config.
        assert!(Arc::ptr_eq(&active.load_full(), &second));
    }

    #[test]
    fn should_retry_respects_max_retries() {
        let runtime = build_runtime(1, 2);
        let proxy = build_proxy(runtime.clone());

        let mut ctx = RequestCtx {
            snapshot: Some(runtime),
            route_idx: Some(0),
            service_idx: Some(0),
            ..RequestCtx::default()
        };

        assert!(proxy.should_retry(&mut ctx));
        assert_eq!(ctx.retries, 1);
        assert!(!proxy.should_retry(&mut ctx));
    }

    #[test]
    fn should_retry_stops_when_all_upstreams_already_attempted() {
        let runtime = build_runtime(3, 2);
        let proxy = build_proxy(runtime.clone());

        let mut ctx = RequestCtx {
            snapshot: Some(runtime),
            route_idx: Some(0),
            service_idx: Some(0),
            attempted_upstreams: vec![0, 1],
            ..RequestCtx::default()
        };

        assert!(!proxy.should_retry(&mut ctx));
        assert_eq!(ctx.retries, 0);
    }
}
