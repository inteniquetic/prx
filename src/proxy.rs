use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use bytes::Bytes;
use pingora::prelude::*;
use tracing::{debug, error, info, warn};

use crate::metrics;
use crate::runtime::{RuntimeConfig, hash_key, normalize_host};

pub struct PrxProxy {
    active_config: Arc<ArcSwap<RuntimeConfig>>,
    access_log: bool,
    health_path: String,
    ready_path: String,
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

        if ctx.retries >= route.max_retries {
            return false;
        }
        if ctx.attempted_upstreams.len() >= route.upstreams.len() {
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
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return;
        };
        let Some(upstream) = route.upstreams.get(upstream_idx) else {
            return;
        };

        metrics::inc_upstream_error(route.name.as_str(), upstream.addr.as_str(), stage);
        let opened = route.mark_upstream_failure(upstream_idx);
        let is_open = upstream.is_circuit_open();
        metrics::set_circuit_state(route.name.as_str(), upstream.addr.as_str(), is_open);
        if opened {
            metrics::mark_circuit_open(route.name.as_str(), upstream.addr.as_str());
            warn!(
                route = route.name.as_str(),
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
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return;
        };
        let Some(upstream) = route.upstreams.get(upstream_idx) else {
            return;
        };

        route.mark_upstream_success(upstream_idx);
        metrics::set_circuit_state(route.name.as_str(), upstream.addr.as_str(), false);
    }
}

pub struct RequestCtx {
    started_at: Instant,
    snapshot: Option<Arc<RuntimeConfig>>,
    route_idx: Option<usize>,
    attempted_upstreams: Vec<usize>,
    retries: usize,
    hash_seed: Option<u64>,
    host: String,
    path: String,
    route_name: Option<String>,
    upstream_addr: Option<String>,
}

impl Default for RequestCtx {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            snapshot: None,
            route_idx: None,
            attempted_upstreams: Vec::new(),
            retries: 0,
            hash_seed: None,
            host: String::new(),
            path: String::new(),
            route_name: None,
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
        let snapshot = self.active_config.load_full();
        ctx.snapshot = Some(snapshot.clone());

        let req_header = session.req_header();
        let host = req_header
            .headers
            .get("host")
            .and_then(|val| val.to_str().ok())
            .map(normalize_host)
            .unwrap_or_else(|| "localhost".to_string());
        let path = req_header.uri.path().to_string();

        ctx.host = host;
        ctx.path = path;
        ctx.hash_seed = Some(hash_key(&[ctx.host.as_str(), ctx.path.as_str()]));

        if ctx.path == self.health_path {
            ctx.route_name = Some("health".to_string());
            return Self::respond_text(session, 200, "ok\n").await;
        }
        if ctx.path == self.ready_path {
            let ready = snapshot.is_ready();
            ctx.route_name = Some("ready".to_string());
            if ready {
                return Self::respond_text(session, 200, "ready\n").await;
            }
            return Self::respond_text(session, 503, "not_ready\n").await;
        }

        ctx.route_idx = snapshot.select_route(&ctx.host, &ctx.path);

        if let Some(route_idx) = ctx.route_idx {
            if let Some(route) = snapshot.route(route_idx) {
                ctx.route_name = Some(route.name.clone());
                debug!(
                    route = %route.name,
                    host = %ctx.host,
                    path = %ctx.path,
                    "matched route"
                );
            }
        } else {
            ctx.route_name = Some("no_route".to_string());
            warn!(host = %ctx.host, path = %ctx.path, "no route matched");
            session.respond_error(404).await?;
            return Ok(true);
        }

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let snapshot = if let Some(snapshot) = &ctx.snapshot {
            snapshot.clone()
        } else {
            let snapshot = self.active_config.load_full();
            ctx.snapshot = Some(snapshot.clone());
            snapshot
        };

        let route_idx = match ctx.route_idx {
            Some(idx) => idx,
            None => {
                return Error::e_explain(
                    HTTPStatus(404),
                    format!("no route matched host={} path={}", ctx.host, ctx.path),
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

        if ctx.retries > 0 && route.retry_backoff_ms > 0 {
            tokio::time::sleep(Duration::from_millis(route.retry_backoff_ms)).await;
        }

        let hash_seed = ctx
            .hash_seed
            .unwrap_or_else(|| hash_key(&[ctx.host.as_str(), ctx.path.as_str()]));
        let (upstream_idx, upstream) =
            if let Some(selected) = route.next_upstream(hash_seed, &ctx.attempted_upstreams) {
                selected
            } else {
                ctx.attempted_upstreams.clear();
                if let Some(selected) = route.next_upstream(hash_seed, &ctx.attempted_upstreams) {
                    selected
                } else {
                    return Error::e_explain(
                        InternalError,
                        format!("route '{}' has no selectable upstreams", route.name),
                    );
                }
            };
        ctx.attempted_upstreams.push(upstream_idx);
        ctx.upstream_addr = Some(upstream.addr.clone());

        let mut peer = HttpPeer::new(upstream.addr.clone(), upstream.tls, upstream.sni.clone());
        peer.options.verify_cert = upstream.verify_cert;
        peer.options.verify_hostname = upstream.verify_hostname;
        if let Some(ms) = upstream.connect_timeout_ms {
            peer.options.connection_timeout = Some(Duration::from_millis(ms));
        }
        if let Some(ms) = upstream.total_connect_timeout_ms {
            peer.options.total_connection_timeout = Some(Duration::from_millis(ms));
        }
        if let Some(ms) = upstream.read_timeout_ms {
            peer.options.read_timeout = Some(Duration::from_millis(ms));
        }
        if let Some(ms) = upstream.write_timeout_ms {
            peer.options.write_timeout = Some(Duration::from_millis(ms));
        }
        if let Some(ms) = upstream.idle_timeout_ms {
            peer.options.idle_timeout = Some(Duration::from_millis(ms));
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
        let Some(upstream_idx) = ctx.attempted_upstreams.last().copied() else {
            return Ok(());
        };
        let Some(upstream) = route.upstreams.get(upstream_idx) else {
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
        if self.should_retry(ctx) {
            e.set_retry(true);
        }
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
        if self.should_retry(ctx) {
            e.set_retry(true);
        }
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
                .unwrap_or_else(|| "unknown".to_string())
        });
        let status = session
            .response_written()
            .map(|resp| resp.status.as_u16())
            .unwrap_or_else(|| if e.is_some() { 500 } else { 0 });
        metrics::observe_request(route_name.as_str(), status, latency_ms as f64);

        if let Some(err) = e {
            error!(
                route = route_name,
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
            route = route_name,
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
        ServerConfig, UpstreamConfig,
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

    fn build_runtime(max_retries: usize, upstream_count: usize) -> Arc<RuntimeConfig> {
        let upstreams = (0..upstream_count)
            .map(|idx| upstream(&format!("127.0.0.1:{}", 9000 + idx)))
            .collect::<Vec<_>>();

        Arc::new(RuntimeConfig::from_config(PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            routes: vec![RouteConfig {
                name: "default".to_string(),
                host: None,
                path_prefix: "/".to_string(),
                is_default: true,
                lb: LbStrategy::RoundRobin,
                max_retries,
                retry_backoff_ms: 0,
                circuit_breaker: CircuitBreakerConfig::default(),
                upstreams,
            }],
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

    #[test]
    fn should_retry_respects_max_retries() {
        let runtime = build_runtime(1, 2);
        let proxy = build_proxy(runtime.clone());

        let mut ctx = RequestCtx {
            snapshot: Some(runtime),
            route_idx: Some(0),
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
            attempted_upstreams: vec![0, 1],
            ..RequestCtx::default()
        };

        assert!(!proxy.should_retry(&mut ctx));
        assert_eq!(ctx.retries, 0);
    }
}
