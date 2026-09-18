use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use bytes::Bytes;
use http::Method;
use pingora::prelude::*;
use pingora::upstreams::peer::ALPN;
use tracing::{debug, error, info, warn};

use crate::config::{LbStrategy, StickyMode, UpstreamH2};
use crate::headers::HeaderContext;
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

    /// True when the request has used up the total budget its service allows.
    fn deadline_exceeded(&self, ctx: &RequestCtx) -> bool {
        let Some(snapshot) = &ctx.snapshot else {
            return false;
        };
        let Some(route) = ctx.route_idx.and_then(|idx| snapshot.route(idx)) else {
            return false;
        };
        let Some(service) = snapshot.service(route.service_idx) else {
            return false;
        };
        service.request_timeout_ms > 0
            && ctx.started_at.elapsed() >= Duration::from_millis(service.request_timeout_ms)
    }

    /// Reports a spent time budget as 504 rather than letting the underlying
    /// read timeout surface as a 502: the upstream was reachable, prx is the
    /// one that stopped waiting.
    fn timeout_error(&self, ctx: &RequestCtx) -> Box<Error> {
        if let Some(route) = ctx
            .snapshot
            .as_ref()
            .and_then(|snapshot| ctx.route_idx.and_then(|idx| snapshot.route(idx)))
        {
            metrics::inc_request_timeout(route.name.as_ref());
        }
        Error::explain(
            HTTPStatus(504),
            "request exceeded the service request_timeout_ms",
        )
    }

    /// Decides whether this request may be retried.
    ///
    /// `stage` separates a connect failure, where nothing reached the upstream
    /// and any method can safely be replayed, from a failure mid-proxy, where
    /// the request may already have been applied.
    fn should_retry(&self, ctx: &mut RequestCtx, stage: RetryStage) -> bool {
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
        let route_name = route.name.as_ref();

        if ctx.retries >= service.max_retries {
            return false;
        }
        if ctx.attempted_upstreams.len() >= service.upstreams.len() {
            return false;
        }

        // A request that already spent its budget must not start another
        // attempt; the client is about to get a 504 either way.
        if service.request_timeout_ms > 0
            && ctx.started_at.elapsed() >= Duration::from_millis(service.request_timeout_ms)
        {
            metrics::inc_retry_denied(route_name, "deadline");
            return false;
        }

        if stage == RetryStage::Proxy && service.retry_idempotent_only && !ctx.is_idempotent {
            metrics::inc_retry_denied(route_name, "not_idempotent");
            return false;
        }

        if !service.retry_budget.try_acquire() {
            metrics::inc_retry_denied(route_name, "budget");
            return false;
        }

        metrics::inc_retry(route_name, stage.as_str());
        ctx.retries += 1;
        true
    }

    async fn respond_text(session: &mut Session, status: u16, body: &'static str) -> Result<bool> {
        session
            .respond_error_with_body(status, Bytes::from_static(body.as_bytes()))
            .await?;
        Ok(true)
    }

    /// Resolves the client address once per request, and only when a header
    /// rule asks for it.
    fn fill_client_addr(session: &Session, ctx: &mut RequestCtx) {
        if ctx.client_ip.is_some() {
            return;
        }
        let Some(addr) = session.client_addr() else {
            return;
        };
        match addr.as_inet() {
            Some(inet) => {
                ctx.client_ip = Some(inet.ip().to_string());
                ctx.client_port = Some(inet.port());
            }
            None => ctx.client_ip = Some(addr.to_string()),
        }
    }

    /// Reuses an inbound `X-Request-Id` when present so a trace keeps one id
    /// across hops, otherwise mints one.
    fn request_id(request: &RequestHeader) -> String {
        if let Some(existing) = request
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= 128)
        {
            return existing.to_string();
        }
        format!("{:032x}", rand::random::<u128>())
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

/// Reads the affinity key for this request: the cookie value in cookie mode,
/// or a hash of the client address or header value in the other modes.
fn sticky_key(session: &Session, sticky: &crate::config::StickyConfig) -> Option<u64> {
    match sticky.mode {
        StickyMode::Cookie => {
            let cookies = session.req_header().headers.get("cookie")?.to_str().ok()?;
            let value = cookies.split(';').find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name.trim() == sticky.name).then(|| value.trim())
            })?;
            u64::from_str_radix(value, 16).ok()
        }
        StickyMode::ClientIp => {
            let addr = session.client_addr()?;
            let ip = match addr.as_inet() {
                Some(inet) => inet.ip().to_string(),
                None => addr.to_string(),
            };
            Some(hash_key(&[ip.as_str()]))
        }
        StickyMode::Header => {
            let value = session
                .req_header()
                .headers
                .get(sticky.name.as_str())?
                .to_str()
                .ok()?;
            Some(hash_key(&[value]))
        }
    }
}

/// Shortens a per-attempt timeout so it cannot outlive the request's total
/// budget. `None` means the service has no total budget.
fn clamp_to_budget(timeout: Duration, remaining: Option<Duration>) -> Duration {
    match remaining {
        Some(remaining) => timeout.min(remaining),
        None => timeout,
    }
}

/// Where a failure happened, which decides whether a replay is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryStage {
    /// The connection to the upstream never came up: nothing was sent.
    Connect,
    /// The request was in flight when it failed.
    Proxy,
}

impl RetryStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Proxy => "proxy",
        }
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
    /// Client address, rendered once per request only when a header rule needs
    /// it (`$client_ip` / `$client_port`).
    client_ip: Option<String>,
    client_port: Option<u16>,
    /// Only generated when a header rule uses `$request_id`.
    request_id: Option<String>,
    /// Whether the request method is safe to replay once it may have reached
    /// the upstream.
    is_idempotent: bool,
    /// Set when a sticky cookie should be written on the way back, holding the
    /// upstream identifier to store.
    sticky_cookie: Option<u64>,
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
            client_ip: None,
            client_port: None,
            request_id: None,
            is_idempotent: true,
            sticky_cookie: None,
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
            ctx.is_idempotent = matches!(
                req_header.method,
                Method::GET
                    | Method::HEAD
                    | Method::OPTIONS
                    | Method::TRACE
                    | Method::PUT
                    | Method::DELETE
            );

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

        // A request with a total budget must not start an attempt it cannot
        // finish, and each attempt is capped by whatever time is left.
        let remaining = if service.request_timeout_ms > 0 {
            let budget = Duration::from_millis(service.request_timeout_ms);
            let elapsed = ctx.started_at.elapsed();
            if elapsed >= budget {
                metrics::inc_request_timeout(route.name.as_ref());
                return Error::e_explain(
                    HTTPStatus(504),
                    format!(
                        "request exceeded request_timeout_ms={} for service '{}'",
                        service.request_timeout_ms, service.name
                    ),
                );
            }
            Some(budget - elapsed)
        } else {
            None
        };

        if ctx.retries > 0 && service.retry_backoff_ms > 0 {
            // Full jitter: a fixed backoff makes every request that failed at
            // the same moment come back at the same moment.
            let backoff = rand::random_range(0..=service.retry_backoff_ms);
            tokio::time::sleep(Duration::from_millis(backoff)).await;
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
        // Session affinity is a preference, never a requirement: when the
        // pinned upstream is gone or unhealthy the request falls back to normal
        // balancing rather than failing.
        let sticky_idx = if service.sticky.enabled {
            let key = sticky_key(session, &service.sticky);
            match service.sticky.mode {
                StickyMode::Cookie => {
                    let pinned =
                        key.and_then(|hash| service.select_sticky(hash, &ctx.attempted_upstreams));
                    if pinned.is_none() {
                        // Remember to hand out a cookie for whatever we pick.
                        ctx.sticky_cookie = Some(0);
                    }
                    pinned
                }
                StickyMode::ClientIp | StickyMode::Header => {
                    key.and_then(|hash| service.select_by_hash(hash, &ctx.attempted_upstreams))
                }
            }
        } else {
            None
        };

        let (upstream_idx, upstream) = if let Some(idx) =
            sticky_idx.and_then(|idx| service.upstreams.get(idx).map(|upstream| (idx, upstream)))
        {
            idx
        } else if let Some(selected) = service.next_upstream(hash_seed, &ctx.attempted_upstreams) {
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
        upstream.inc_inflight();
        if ctx.sticky_cookie == Some(0) {
            ctx.sticky_cookie = Some(upstream.addr_hash());
        }

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
            peer.options.connection_timeout =
                Some(clamp_to_budget(Duration::from_millis(ms), remaining));
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
                peer.options.read_timeout =
                    Some(clamp_to_budget(Duration::from_millis(ms), remaining));
            }
            if let Some(ms) = upstream.write_timeout_ms {
                peer.options.write_timeout =
                    Some(clamp_to_budget(Duration::from_millis(ms), remaining));
            }
            if let Some(ms) = upstream.idle_timeout_ms {
                peer.options.idle_timeout = Some(Duration::from_millis(ms));
            }
        }

        Ok(Box::new(peer))
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Phase 1: everything that needs the snapshot, ending with Copy values
        // so ctx can be written to afterwards without cloning the rule set.
        let needs = {
            let Some(snapshot) = &ctx.snapshot else {
                return Ok(());
            };
            let Some(route) = ctx.route_idx.and_then(|idx| snapshot.route(idx)) else {
                return Ok(());
            };
            let Some(service) = snapshot.service(route.service_idx) else {
                return Ok(());
            };
            let Some(upstream) = ctx
                .attempted_upstreams
                .last()
                .copied()
                .and_then(|idx| service.upstreams.get(idx))
            else {
                return Ok(());
            };

            // Keep Host aligned with SNI when proxying to strict virtual hosts.
            // Route rules run after this, so a rule can still override Host.
            upstream_request.insert_header("host", upstream.sni.as_str())?;
            if route.request_headers.is_empty() {
                None
            } else {
                Some((
                    route.request_headers.needs_client_addr(),
                    route.request_headers.needs_request_id(),
                ))
            }
        };

        // Phase 2: fill in what the rules ask for, and only that.
        if let Some((needs_client_addr, needs_request_id)) = needs {
            if needs_client_addr {
                Self::fill_client_addr(session, ctx);
            }
            if needs_request_id && ctx.request_id.is_none() {
                ctx.request_id = Some(Self::request_id(upstream_request));
            }

            // Phase 3: apply. Only immutable borrows of ctx from here.
            let host = session
                .req_header()
                .headers
                .get("host")
                .and_then(|value| value.to_str().ok());
            if let Some(snapshot) = &ctx.snapshot
                && let Some(route) = ctx.route_idx.and_then(|idx| snapshot.route(idx))
            {
                {
                    let scheme = snapshot
                        .service(route.service_idx)
                        .and_then(|service| {
                            ctx.attempted_upstreams
                                .last()
                                .copied()
                                .and_then(|idx| service.upstreams.get(idx))
                        })
                        .map(|upstream| if upstream.tls { "https" } else { "http" })
                        .unwrap_or("http");

                    let header_ctx = HeaderContext {
                        client_ip: ctx.client_ip.as_deref(),
                        client_port: ctx.client_port,
                        scheme,
                        host,
                        route_name: Some(route.name.as_ref()),
                        upstream_addr: ctx.upstream_addr.as_deref(),
                        request_id: ctx.request_id.as_deref(),
                    };
                    route.request_headers.apply(upstream_request, &header_ctx);
                }
            }
        }

        self.record_upstream_success(ctx);
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let Some(snapshot) = &ctx.snapshot else {
            return Ok(());
        };
        let Some(route) = ctx.route_idx.and_then(|idx| snapshot.route(idx)) else {
            return Ok(());
        };

        // Hand out the affinity cookie for the upstream this request landed on.
        if let Some(addr_hash) = ctx.sticky_cookie.filter(|hash| *hash != 0)
            && let Some(service) = snapshot.service(route.service_idx)
            && service.sticky.enabled
            && service.sticky.mode == StickyMode::Cookie
        {
            let cookie = format!(
                "{}={:x}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
                service.sticky.name, addr_hash, service.sticky.ttl_s
            );
            let _ = upstream_response.append_header("set-cookie", cookie);
        }

        if route.response_headers.is_empty() {
            return Ok(());
        }

        let header_ctx = HeaderContext {
            client_ip: ctx.client_ip.as_deref(),
            client_port: ctx.client_port,
            scheme: "http",
            host: None,
            route_name: Some(route.name.as_ref()),
            upstream_addr: ctx.upstream_addr.as_deref(),
            request_id: ctx.request_id.as_deref(),
        };
        route.response_headers.apply(upstream_response, &header_ctx);
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
        if self.deadline_exceeded(ctx) {
            return self.timeout_error(ctx);
        }
        e.set_retry(self.should_retry(ctx, RetryStage::Connect));
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
        if self.deadline_exceeded(ctx) {
            return self.timeout_error(ctx);
        }
        e.set_retry(self.should_retry(ctx, RetryStage::Proxy));
        e
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX) {
        let elapsed = ctx.started_at.elapsed();
        let latency_ms = elapsed.as_millis();

        // Every attempt took a slot; give them all back exactly once, and feed
        // the latency of the attempt that actually served the request into the
        // moving average used by p2c_ewma.
        if let Some(snapshot) = &ctx.snapshot
            && let Some(route) = ctx.route_idx.and_then(|idx| snapshot.route(idx))
            && let Some(service) = snapshot.service(route.service_idx)
        {
            let last = ctx.attempted_upstreams.last().copied();
            for idx in &ctx.attempted_upstreams {
                if let Some(upstream) = service.upstreams.get(*idx) {
                    upstream.dec_inflight();
                    if Some(*idx) == last && e.is_none() {
                        upstream.record_latency(elapsed.as_micros() as u64);
                    }
                    metrics::set_upstream_inflight(
                        service.name.as_str(),
                        upstream.addr.as_str(),
                        upstream.inflight(),
                    );
                    metrics::set_upstream_ewma_ms(
                        service.name.as_str(),
                        upstream.addr.as_str(),
                        upstream.ewma_us() as f64 / 1000.0,
                    );
                }
            }
            if e.is_none() {
                service.retry_budget.record_success();
            }
        }

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

        // Metrics are recorded whether or not the access log is on: they are
        // separate signals, and tying them together silently emptied /metrics
        // for anyone running with access_log = false.
        metrics::observe_request(route_name.as_ref(), status, latency_ms as f64);

        if !self.access_log {
            return;
        }
        let summary = session.request_summary();

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
            ..Default::default()
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
            request_headers: Default::default(),
            response_headers: Default::default(),
        }
    }

    fn build_runtime(max_retries: usize, upstream_count: usize) -> Arc<RuntimeConfig> {
        Arc::new(RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
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

    fn runtime_with(service: ServiceConfig) -> Arc<RuntimeConfig> {
        Arc::new(RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
            services: vec![service],
            routes: vec![route("default", "default")],
        }))
    }

    fn ctx_for(runtime: &Arc<RuntimeConfig>) -> RequestCtx {
        RequestCtx {
            snapshot: Some(runtime.clone()),
            route_idx: Some(0),
            service_idx: Some(0),
            ..RequestCtx::default()
        }
    }

    /// T107: replaying a POST after the request may already have reached the
    /// upstream can apply it twice.
    #[test]
    fn a_non_idempotent_request_is_not_replayed_after_a_proxy_error() {
        let runtime = runtime_with(ServiceConfig {
            max_retries: 3,
            retry_idempotent_only: true,
            retry_budget_ratio: 0.0,
            upstreams: vec![upstream("127.0.0.1:9000"), upstream("127.0.0.1:9001")],
            ..Default::default()
        });
        let proxy = build_proxy(runtime.clone());

        let mut post = ctx_for(&runtime);
        post.is_idempotent = false;
        assert!(
            !proxy.should_retry(&mut post, RetryStage::Proxy),
            "a POST must not be replayed once it may have been applied"
        );

        // The same POST is safe to retry when the connection never came up.
        assert!(proxy.should_retry(&mut post, RetryStage::Connect));

        let mut get = ctx_for(&runtime);
        get.is_idempotent = true;
        assert!(proxy.should_retry(&mut get, RetryStage::Proxy));
    }

    /// T107: with the budget disabled the old behavior is preserved.
    #[test]
    fn retry_budget_can_be_turned_off() {
        let runtime = runtime_with(ServiceConfig {
            max_retries: 1,
            retry_budget_ratio: 0.0,
            upstreams: vec![upstream("127.0.0.1:9000"), upstream("127.0.0.1:9001")],
            ..Default::default()
        });
        let proxy = build_proxy(runtime.clone());

        let mut ctx = ctx_for(&runtime);
        assert!(proxy.should_retry(&mut ctx, RetryStage::Connect));
    }

    /// T107: a failing upstream must not receive `max_retries` times the load.
    #[test]
    fn retry_budget_limits_a_storm_of_retries() {
        let runtime = runtime_with(ServiceConfig {
            max_retries: 3,
            retry_budget_ratio: 0.1,
            retry_budget_min_per_window: 5,
            retry_budget_window_ms: 60_000,
            upstreams: vec![upstream("127.0.0.1:9000"), upstream("127.0.0.1:9001")],
            ..Default::default()
        });
        let proxy = build_proxy(runtime.clone());

        // No successes recorded yet, so only the floor of 5 retries is allowed
        // no matter how many requests ask for one.
        let mut granted = 0;
        for _ in 0..100 {
            let mut ctx = ctx_for(&runtime);
            if proxy.should_retry(&mut ctx, RetryStage::Connect) {
                granted += 1;
            }
        }
        assert_eq!(
            granted, 5,
            "the budget floor must cap retries at 5, got {granted}"
        );

        // 100 successes raise the allowance to 10% of them.
        let service = runtime.service(0).expect("service 0 exists");
        for _ in 0..100 {
            service.retry_budget.record_success();
        }
        let mut granted_after = 0;
        for _ in 0..100 {
            let mut ctx = ctx_for(&runtime);
            if proxy.should_retry(&mut ctx, RetryStage::Connect) {
                granted_after += 1;
            }
        }
        assert_eq!(
            granted_after, 5,
            "10% of 100 successes is 10 retries in total, 5 of which were already spent"
        );
    }

    /// T107: once the total budget is gone, starting another attempt only
    /// delays the 504 the client is going to get.
    #[test]
    fn a_request_past_its_deadline_is_not_retried() {
        let runtime = runtime_with(ServiceConfig {
            max_retries: 3,
            request_timeout_ms: 1,
            retry_budget_ratio: 0.0,
            upstreams: vec![upstream("127.0.0.1:9000"), upstream("127.0.0.1:9001")],
            ..Default::default()
        });
        let proxy = build_proxy(runtime.clone());

        let mut ctx = ctx_for(&runtime);
        ctx.started_at = Instant::now() - Duration::from_millis(50);
        assert!(!proxy.should_retry(&mut ctx, RetryStage::Connect));
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

        assert!(proxy.should_retry(&mut ctx, RetryStage::Connect));
        assert_eq!(ctx.retries, 1);
        assert!(!proxy.should_retry(&mut ctx, RetryStage::Connect));
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

        assert!(!proxy.should_retry(&mut ctx, RetryStage::Connect));
        assert_eq!(ctx.retries, 0);
    }
}
