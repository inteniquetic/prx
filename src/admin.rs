use std::{
    convert::Infallible,
    fs::{self, File, OpenOptions},
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::{
    Router,
    body::{self, Body},
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use include_dir::{Dir, include_dir};
use pingora::services::Service;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    config::{
        CacheConfig, ConcurrencyLimitConfig, HeaderRules, HealthCheckConfig, LbStrategy, PrxConfig,
        RateLimitConfig, RouteConfig, ServiceConfig, StickyConfig, UpstreamConfig, UpstreamH2,
    },
    router::RouteMatch,
    runtime::RuntimeConfig,
    stats::{self, EventLevel},
    validate::{self, ValidationReport},
};

pub const ADMIN_CONFIG_PATH: &str = "/web/config";
pub const ADMIN_CONFIG_VALIDATE_PATH: &str = "/web/config/validate";
pub const ADMIN_ROUTE_HEALTH_PATH: &str = "/web/health/routes";
pub const ADMIN_CACHE_PATH: &str = "/web/cache";
pub const ADMIN_TLS_STATUS_PATH: &str = "/web/tls/status";
pub const DEFAULT_ADMIN_LISTEN: &str = "127.0.0.1:9090";
const MAX_ADMIN_CONFIG_BODY_BYTES: usize = 10 * 1024 * 1024;
pub const ADMIN_SERVICES_PATH: &str = "/admin/services";
pub const ADMIN_SERVICES_NAME_PATH: &str = "/admin/services/{name}";
pub const ADMIN_ROUTES_PATH: &str = "/admin/routes";
pub const ADMIN_ROUTES_NAME_PATH: &str = "/admin/routes/{name}";
pub const ADMIN_ROUTE_TEST_PATH: &str = "/web/routes/test";
pub const ADMIN_SERVICE_STATUS_PATH: &str = "/web/services/status";
pub const ADMIN_UPSTREAM_TEST_PATH: &str = "/web/upstreams/test";
pub const ADMIN_STATS_PATH: &str = "/web/stats";
pub const ADMIN_STATS_STREAM_PATH: &str = "/web/stats/stream";
const WEBUI_INDEX_PATH: &str = "index.html";
static WEBUI_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/webui/dist");

#[derive(Clone)]
pub struct ConfigAdmin {
    config_path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl ConfigAdmin {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn read_config_text(&self) -> anyhow::Result<String> {
        fs::read_to_string(&self.config_path).with_context(|| {
            format!(
                "failed to read config file at {}",
                self.config_path.to_string_lossy()
            )
        })
    }

    pub fn read_parsed_config(&self) -> anyhow::Result<PrxConfig> {
        PrxConfig::from_file(&self.config_path)
    }

    pub fn apply_config_text(
        &self,
        toml_text: &str,
        active_config: &Arc<ArcSwap<RuntimeConfig>>,
    ) -> anyhow::Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("config write lock is poisoned"))?;

        let previous_bytes = fs::read(&self.config_path).with_context(|| {
            format!(
                "failed to read previous config at {}",
                self.config_path.to_string_lossy()
            )
        })?;

        Self::atomic_replace(&self.config_path, toml_text.as_bytes()).with_context(|| {
            format!(
                "failed to atomically write config to {}",
                self.config_path.to_string_lossy()
            )
        })?;

        match PrxConfig::from_file(&self.config_path) {
            Ok(verified) => {
                active_config.store(Arc::new(RuntimeConfig::from_config(verified)));
                Ok(())
            }
            Err(err) => {
                let rollback_result = Self::atomic_replace(&self.config_path, &previous_bytes)
                    .with_context(|| {
                        format!(
                            "failed to rollback config at {}",
                            self.config_path.to_string_lossy()
                        )
                    });

                if let Err(rollback_err) = rollback_result {
                    bail!(
                        "config write verification failed: {err:#}; rollback failed: {rollback_err:#}"
                    );
                }

                if let Ok(rolled_back) = PrxConfig::from_file(&self.config_path) {
                    active_config.store(Arc::new(RuntimeConfig::from_config(rolled_back)));
                }

                bail!("config write verification failed, rolled back previous config: {err:#}");
            }
        }
    }

    pub fn modify_config<F>(
        &self,
        active_config: &Arc<ArcSwap<RuntimeConfig>>,
        f: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(&mut PrxConfig) -> anyhow::Result<()>,
    {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("config write lock is poisoned"))?;

        // Fail early when the current config is unreadable, before anything is
        // modified. Keeping the bytes around for rollback is T204's job.
        fs::read(&self.config_path).with_context(|| {
            format!(
                "failed to read previous config at {}",
                self.config_path.to_string_lossy()
            )
        })?;

        // Read, modify, and validate
        let mut config = PrxConfig::from_file(&self.config_path).with_context(|| {
            format!(
                "failed to read config at {}",
                self.config_path.to_string_lossy()
            )
        })?;

        f(&mut config)?;

        config.validate().with_context(|| {
            format!(
                "modified config at {} failed validation",
                self.config_path.to_string_lossy()
            )
        })?;

        // Serialize to TOML
        let toml_text = toml::to_string(&config).context("failed to serialize config to TOML")?;

        // Atomically write the new config
        Self::atomic_replace(&self.config_path, toml_text.as_bytes()).with_context(|| {
            format!(
                "failed to atomically write config to {}",
                self.config_path.to_string_lossy()
            )
        })?;

        // Update the active config
        active_config.store(Arc::new(RuntimeConfig::from_config(config)));

        Ok(())
    }

    fn atomic_replace(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Prx.toml");

        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let temp_path = parent.join(format!(".{file_name}.tmp-{}-{now_ns}", std::process::id()));

        {
            let mut temp_file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)
                .with_context(|| {
                    format!(
                        "failed to create temp config file at {}",
                        temp_path.to_string_lossy()
                    )
                })?;
            temp_file
                .write_all(bytes)
                .context("failed to write temp config")?;
            temp_file
                .sync_all()
                .context("failed to fsync temp config")?;
        }

        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to replace config {} with {}",
                path.to_string_lossy(),
                temp_path.to_string_lossy()
            )
        })?;

        // Best effort fsync on parent dir to persist rename metadata.
        if let Ok(parent_dir) = File::open(&parent) {
            let _ = parent_dir.sync_all();
        }

        Ok(())
    }
}

#[derive(Clone)]
struct AdminState {
    config_admin: ConfigAdmin,
    active_config: Arc<ArcSwap<RuntimeConfig>>,
    acme_status: crate::acme::SharedStatus,
    /// Present when a TLS listener is configured.
    tls_resolver: Option<Arc<crate::tls::CertResolver>>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigQuery {
    format: Option<String>,
    /// `PUT ?dry_run=true` validates and answers with the report, and writes
    /// nothing (T203).
    dry_run: Option<String>,
}

/// Loose truthiness for a query flag: `?dry_run` on its own means yes, and so
/// do `1`, `true` and `yes`, because every one of them gets typed.
fn query_flag(value: Option<&String>) -> bool {
    match value {
        None => false,
        Some(raw) => {
            let raw = raw.trim();
            raw.is_empty()
                || raw.eq_ignore_ascii_case("true")
                || raw.eq_ignore_ascii_case("1")
                || raw.eq_ignore_ascii_case("yes")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RouteHealthQuery {
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct AdminConfigPayload {
    server: AdminServerPayload,
    observability: AdminObservabilityPayload,
    services: Vec<AdminServicePayload>,
    routes: Vec<AdminRoutePayload>,
}

#[derive(Debug, Serialize)]
struct AdminServerPayload {
    listen: Vec<String>,
    health_path: String,
    ready_path: String,
    threads: Option<usize>,
    grace_period_seconds: Option<u64>,
    graceful_shutdown_timeout_seconds: Option<u64>,
    config_reload_debounce_ms: u64,
    tls: Option<AdminTlsPayload>,
}

#[derive(Debug, Serialize)]
struct AdminTlsPayload {
    listen: String,
    /// Present only for the single-certificate form.
    cert_path: Option<String>,
    key_path: Option<String>,
    enable_h2: bool,
    /// Every certificate this listener can serve, including the single-cert
    /// form, so the UI shows one consistent list.
    certs: Vec<AdminTlsCertPayload>,
}

#[derive(Debug, Serialize)]
struct AdminTlsCertPayload {
    domains: Vec<String>,
    cert_path: String,
    key_path: String,
    is_default: bool,
}

#[derive(Debug, Serialize)]
struct AdminObservabilityPayload {
    log_level: String,
    access_log: bool,
    prometheus_listen: String,
}

/// A service exactly as the config holds it.
///
/// Same reasoning as `AdminRoutePayload`: the UI is the only editor most people
/// will use, so a payload that leaves out health checks, session affinity or
/// retry budgets turns "edit this service" into "lose them".
#[derive(Debug, Serialize)]
struct AdminServicePayload {
    name: String,
    lb: String,
    max_retries: usize,
    retry_backoff_ms: u64,
    retry_budget_ratio: f64,
    retry_budget_min_per_window: u64,
    retry_budget_window_ms: u64,
    retry_idempotent_only: bool,
    request_timeout_ms: u64,
    upstream_h2: UpstreamH2,
    circuit_breaker: AdminCircuitBreakerPayload,
    health_check: HealthCheckConfig,
    sticky: StickyConfig,
    upstreams: Vec<AdminUpstreamPayload>,
}

impl AdminServicePayload {
    fn from_config(service: &ServiceConfig) -> Self {
        Self {
            name: service.name.clone(),
            lb: lb_to_string(service.lb.clone()).to_string(),
            max_retries: service.max_retries,
            retry_backoff_ms: service.retry_backoff_ms,
            retry_budget_ratio: service.retry_budget_ratio,
            retry_budget_min_per_window: service.retry_budget_min_per_window,
            retry_budget_window_ms: service.retry_budget_window_ms,
            retry_idempotent_only: service.retry_idempotent_only,
            request_timeout_ms: service.request_timeout_ms,
            upstream_h2: service.upstream_h2,
            circuit_breaker: AdminCircuitBreakerPayload {
                enabled: service.circuit_breaker.enabled,
                consecutive_failures: service.circuit_breaker.consecutive_failures,
                open_ms: service.circuit_breaker.open_ms,
            },
            health_check: service.health_check.clone(),
            sticky: service.sticky.clone(),
            upstreams: service
                .upstreams
                .iter()
                .map(AdminUpstreamPayload::from_config)
                .collect(),
        }
    }
}

/// A route exactly as the config holds it.
///
/// The advanced blocks travel with it because the UI is the only editor most
/// people will use: a payload that leaves out header rules, limits or cache
/// settings turns "edit this route" into "lose whatever the file said about
/// them".
#[derive(Debug, Serialize)]
struct AdminRoutePayload {
    name: String,
    service: String,
    host: String,
    path_prefix: String,
    methods: Vec<String>,
    is_default: bool,
    enabled: bool,
    request_headers: HeaderRules,
    response_headers: HeaderRules,
    rate_limit: RateLimitConfig,
    concurrency_limit: ConcurrencyLimitConfig,
    cache: CacheConfig,
}

impl AdminRoutePayload {
    fn from_config(route: &RouteConfig) -> Self {
        Self {
            name: route.name.clone(),
            service: route.service.clone(),
            host: route.host.clone().unwrap_or_default(),
            path_prefix: route.path_prefix.clone(),
            methods: route.methods.clone(),
            is_default: route.is_default,
            enabled: route.enabled,
            request_headers: route.request_headers.clone(),
            response_headers: route.response_headers.clone(),
            rate_limit: route.rate_limit.clone(),
            concurrency_limit: route.concurrency_limit.clone(),
            cache: route.cache.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AdminCircuitBreakerPayload {
    enabled: bool,
    consecutive_failures: usize,
    open_ms: u64,
}

#[derive(Debug, Serialize)]
struct AdminUpstreamPayload {
    addr: String,
    enabled: bool,
    tls: bool,
    sni: String,
    weight: u16,
    verify_cert: Option<bool>,
    verify_hostname: Option<bool>,
    connect_timeout_ms: Option<u64>,
    total_connect_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    write_timeout_ms: Option<u64>,
    idle_timeout_ms: Option<u64>,
}

impl AdminUpstreamPayload {
    fn from_config(upstream: &UpstreamConfig) -> Self {
        Self {
            addr: upstream.addr.clone(),
            enabled: upstream.enabled,
            tls: upstream.tls,
            sni: upstream.sni.clone().unwrap_or_default(),
            weight: upstream.weight,
            verify_cert: upstream.verify_cert,
            verify_hostname: upstream.verify_hostname,
            connect_timeout_ms: upstream.connect_timeout_ms,
            total_connect_timeout_ms: upstream.total_connect_timeout_ms,
            read_timeout_ms: upstream.read_timeout_ms,
            write_timeout_ms: upstream.write_timeout_ms,
            idle_timeout_ms: upstream.idle_timeout_ms,
        }
    }
}

// Request payloads for Service CRUD
#[derive(Debug, Deserialize)]
struct ServiceRequestPayload {
    pub name: String,
    #[serde(default)]
    pub lb: Option<String>,
    #[serde(default)]
    pub max_retries: Option<usize>,
    #[serde(default)]
    pub retry_backoff_ms: Option<u64>,
    // Absent means "leave this as it is" on an update, and "take the default"
    // on a create — an older client cannot erase a block it has no field for.
    #[serde(default)]
    pub retry_budget_ratio: Option<f64>,
    #[serde(default)]
    pub retry_budget_min_per_window: Option<u64>,
    #[serde(default)]
    pub retry_budget_window_ms: Option<u64>,
    #[serde(default)]
    pub retry_idempotent_only: Option<bool>,
    #[serde(default)]
    pub request_timeout_ms: Option<u64>,
    #[serde(default)]
    pub upstream_h2: Option<UpstreamH2>,
    #[serde(default)]
    pub circuit_breaker: Option<CircuitBreakerRequestPayload>,
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
    #[serde(default)]
    pub sticky: Option<StickyConfig>,
    #[serde(default)]
    pub upstreams: Vec<UpstreamRequestPayload>,
}

#[derive(Debug, Deserialize)]
struct CircuitBreakerRequestPayload {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub consecutive_failures: Option<usize>,
    #[serde(default)]
    pub open_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct UpstreamRequestPayload {
    pub addr: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub sni: Option<String>,
    #[serde(default)]
    pub weight: Option<u16>,
    #[serde(default)]
    pub verify_cert: Option<bool>,
    #[serde(default)]
    pub verify_hostname: Option<bool>,
    #[serde(default)]
    pub connect_timeout_ms: Option<u64>,
    #[serde(default)]
    pub total_connect_timeout_ms: Option<u64>,
    #[serde(default)]
    pub read_timeout_ms: Option<u64>,
    #[serde(default)]
    pub write_timeout_ms: Option<u64>,
    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,
}

// Request payloads for Route CRUD
#[derive(Debug, Deserialize)]
struct RouteRequestPayload {
    pub name: String,
    pub service: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub methods: Option<Vec<String>>,
    #[serde(default)]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    // Absent means "leave this as it is" on an update, and "take the default"
    // on a create — so a client that does not know about a block cannot erase
    // it.
    #[serde(default)]
    pub request_headers: Option<HeaderRules>,
    #[serde(default)]
    pub response_headers: Option<HeaderRules>,
    #[serde(default)]
    pub rate_limit: Option<RateLimitConfig>,
    #[serde(default)]
    pub concurrency_limit: Option<ConcurrencyLimitConfig>,
    #[serde(default)]
    pub cache: Option<CacheConfig>,
}

/// Live state of every service, as the running proxy sees it right now.
///
/// The config says what was asked for; this says what is happening. The page
/// that shows circuit breakers and probe verdicts needs the second one, and it
/// has to come from the same snapshot traffic is being routed on.
#[derive(Debug, Serialize)]
struct ServiceStatusPayload {
    checked_at_epoch_ms: u64,
    services: Vec<ServiceStatusEntry>,
}

#[derive(Debug, Serialize)]
struct ServiceStatusEntry {
    name: String,
    lb: String,
    /// Whether the background prober is running for this service at all.
    health_check_enabled: bool,
    circuit_breaker_enabled: bool,
    upstreams: Vec<UpstreamStatusEntry>,
}

#[derive(Debug, Serialize)]
struct UpstreamStatusEntry {
    addr: String,
    /// False when the upstream is drained: still configured, taking no traffic.
    enabled: bool,
    weight: u16,
    /// Share of the selection ring, read from the ring itself.
    share: f64,
    available: bool,
    circuit_open: bool,
    /// Milliseconds until the breaker closes again, when it is open.
    circuit_reopens_in_ms: Option<u64>,
    consecutive_failures: usize,
    probe_healthy: bool,
    last_probe_ms_ago: Option<u64>,
    inflight: usize,
    ewma_us: u64,
}

/// One upstream, probed on demand.
#[derive(Debug, Deserialize)]
struct UpstreamTestRequest {
    addr: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// "If a request like this arrived, where would it go?"
#[derive(Debug, Deserialize)]
struct RouteTestRequest {
    #[serde(default = "default_test_method")]
    method: String,
    #[serde(default)]
    host: String,
    #[serde(default = "default_test_path")]
    path: String,
}

fn default_test_method() -> String {
    "GET".to_string()
}

fn default_test_path() -> String {
    "/".to_string()
}

#[derive(Debug, Serialize)]
struct RouteTestResponse {
    /// `matched`, `method_not_allowed` or `not_found`.
    outcome: &'static str,
    /// The request as the matcher saw it, after host normalization.
    request: RouteTestEcho,
    route: Option<RouteTestRoutePayload>,
    service: Option<RouteTestServicePayload>,
}

#[derive(Debug, Serialize)]
struct RouteTestEcho {
    method: String,
    host: String,
    normalized_host: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct RouteTestRoutePayload {
    index: usize,
    name: String,
    host: String,
    path_prefix: String,
    methods: Vec<String>,
    is_default: bool,
    enabled: bool,
    /// Which tier of the matching rules this route won on: `exact_host`,
    /// `wildcard_host`, `any_host`, or `default_route` when it only caught the
    /// request as the fallback.
    matched_by: &'static str,
}

#[derive(Debug, Serialize)]
struct RouteTestServicePayload {
    name: String,
    lb: String,
    selection: RouteTestSelection,
    upstreams: Vec<RouteTestUpstream>,
}

#[derive(Debug, Serialize)]
struct RouteTestSelection {
    /// False for the strategies that draw at random: there is no single
    /// upstream to name, and pretending otherwise would be a guess.
    deterministic: bool,
    would_pick: Option<String>,
    note: String,
}

#[derive(Debug, Serialize)]
struct RouteTestUpstream {
    addr: String,
    weight: u16,
    /// Whether this upstream is in the running for the next request.
    available: bool,
    circuit_open: bool,
    probe_healthy: bool,
    inflight: usize,
    ewma_us: u64,
}

#[derive(Debug, Serialize)]
struct RouteHealthPayload {
    checked_at_epoch_ms: u64,
    timeout_ms: u64,
    routes: Vec<RouteHealthRoutePayload>,
}

#[derive(Debug, Serialize)]
struct RouteHealthRoutePayload {
    route_index: usize,
    name: String,
    service: String,
    host: String,
    path_prefix: String,
    healthy: bool,
    reachable_upstreams: usize,
    total_upstreams: usize,
    upstreams: Vec<RouteHealthUpstreamPayload>,
}

#[derive(Debug, Serialize)]
struct RouteHealthUpstreamPayload {
    addr: String,
    timeout_ms: u64,
    healthy: bool,
    latency_ms: Option<u64>,
    error: Option<String>,
    /// Where this verdict came from: the background prober, or a TCP connect
    /// opened just to answer this request.
    source: &'static str,
    /// How long ago the prober last checked, when the verdict came from it.
    last_probe_ms_ago: Option<u64>,
}

impl From<PrxConfig> for AdminConfigPayload {
    fn from(config: PrxConfig) -> Self {
        let server = AdminServerPayload {
            listen: config.server.listen,
            health_path: config.server.health_path,
            ready_path: config.server.ready_path,
            threads: config.server.threads,
            grace_period_seconds: config.server.grace_period_seconds,
            graceful_shutdown_timeout_seconds: config.server.graceful_shutdown_timeout_seconds,
            config_reload_debounce_ms: config.server.config_reload_debounce_ms,
            tls: config.server.tls.map(|tls| {
                let certs = tls
                    .all_certs()
                    .into_iter()
                    .map(|cert| AdminTlsCertPayload {
                        domains: cert.domains,
                        cert_path: cert.cert_path,
                        key_path: cert.key_path,
                        is_default: cert.is_default,
                    })
                    .collect();
                AdminTlsPayload {
                    listen: tls.listen,
                    cert_path: tls.cert_path,
                    key_path: tls.key_path,
                    enable_h2: tls.enable_h2,
                    certs,
                }
            }),
        };

        let observability = AdminObservabilityPayload {
            log_level: config.observability.log_level,
            access_log: config.observability.access_log,
            prometheus_listen: config.observability.prometheus_listen.unwrap_or_default(),
        };

        let services = config
            .services
            .iter()
            .map(AdminServicePayload::from_config)
            .collect();

        let routes = config
            .routes
            .iter()
            .map(AdminRoutePayload::from_config)
            .collect();

        Self {
            server,
            observability,
            services,
            routes,
        }
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn health_timeout_ms(raw: Option<u64>) -> u64 {
    raw.unwrap_or(1200).clamp(100, 10_000)
}

async fn check_upstream_health(addr: String, timeout_ms: u64) -> RouteHealthUpstreamPayload {
    use tokio::time::{Duration, Instant, timeout};

    if addr.trim().is_empty() {
        return RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: false,
            latency_ms: None,
            error: Some("empty_addr".to_string()),
            source: "tcp_connect",
            last_probe_ms_ago: None,
        };
    }

    let start = Instant::now();
    match timeout(
        Duration::from_millis(timeout_ms),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_stream)) => RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: true,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
            source: "tcp_connect",
            last_probe_ms_ago: None,
        },
        Ok(Err(err)) => RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: false,
            latency_ms: None,
            error: Some(err.to_string()),
            source: "tcp_connect",
            last_probe_ms_ago: None,
        },
        Err(_) => RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: false,
            latency_ms: None,
            error: Some("timeout".to_string()),
            source: "tcp_connect",
            last_probe_ms_ago: None,
        },
    }
}

/// Verdicts the background prober already holds, keyed by upstream address.
type ProbeVerdicts = std::collections::HashMap<String, (bool, u64)>;

/// Collects what the active health checker knows, so the admin API can answer
/// from it instead of opening a TCP connection per upstream on every page load.
fn probe_verdicts(active_config: &Arc<ArcSwap<RuntimeConfig>>) -> ProbeVerdicts {
    let snapshot = active_config.load();
    let mut verdicts = ProbeVerdicts::new();
    for idx in 0..snapshot.service_count() {
        let Some(service) = snapshot.service(idx) else {
            continue;
        };
        if !service.health_check.enabled {
            continue;
        }
        for upstream in &service.upstreams {
            verdicts.insert(
                upstream.addr.clone(),
                (upstream.is_probe_healthy(), upstream.last_probe_ms()),
            );
        }
    }
    verdicts
}

async fn render_route_health_payload(
    config: PrxConfig,
    timeout_ms: u64,
    verdicts: &ProbeVerdicts,
) -> RouteHealthPayload {
    // Build a service lookup map
    let service_map: std::collections::HashMap<String, _> = config
        .services
        .into_iter()
        .map(|svc| (svc.name.clone(), svc))
        .collect();

    let mut route_payloads = Vec::with_capacity(config.routes.len());
    for (route_index, route) in config.routes.into_iter().enumerate() {
        let mut upstream_payloads = Vec::new();

        if let Some(service) = service_map.get(&route.service) {
            for upstream in &service.upstreams {
                // Prefer what the background prober already knows: it reflects
                // the verdict traffic is actually routed on, and it does not
                // open a connection per upstream every time the UI refreshes.
                if let Some((healthy, last_probe_ms)) = verdicts.get(&upstream.addr) {
                    upstream_payloads.push(RouteHealthUpstreamPayload {
                        addr: upstream.addr.clone(),
                        timeout_ms: 0,
                        healthy: *healthy,
                        latency_ms: None,
                        error: None,
                        source: "active_probe",
                        last_probe_ms_ago: (*last_probe_ms > 0)
                            .then(|| now_epoch_ms().saturating_sub(*last_probe_ms)),
                    });
                    continue;
                }

                let per_upstream_timeout_ms = health_timeout_ms(upstream.connect_timeout_ms);
                upstream_payloads.push(
                    check_upstream_health(upstream.addr.clone(), per_upstream_timeout_ms).await,
                );
            }
        }

        let reachable_upstreams = upstream_payloads
            .iter()
            .filter(|upstream| upstream.healthy)
            .count();
        route_payloads.push(RouteHealthRoutePayload {
            route_index,
            name: route.name,
            service: route.service,
            host: route.host.unwrap_or_default(),
            path_prefix: route.path_prefix,
            healthy: reachable_upstreams > 0,
            reachable_upstreams,
            total_upstreams: upstream_payloads.len(),
            upstreams: upstream_payloads,
        });
    }

    RouteHealthPayload {
        checked_at_epoch_ms: now_epoch_ms(),
        timeout_ms,
        routes: route_payloads,
    }
}

#[derive(Debug, Deserialize)]
struct CacheQuery {
    /// Limit the action to one route; omitted means every route.
    route: Option<String>,
}

#[derive(Debug, Serialize)]
struct CacheRoutePayload {
    route: String,
    entries: usize,
    bytes: usize,
    hits: u64,
    misses: u64,
    coalesced: u64,
    evictions: u64,
}

#[derive(Debug, Serialize)]
struct TlsStatusPayload {
    acme: crate::acme::AcmeStatus,
    /// Certificates the running listener would serve right now.
    certificates: Vec<TlsCertStatusPayload>,
}

#[derive(Debug, Serialize)]
struct TlsCertStatusPayload {
    domain: String,
    expires_epoch_s: i64,
    expires_in_days: i64,
}

/// Reports what the TLS listener is actually serving, and how automatic
/// renewal is going, so an expiring certificate is visible before it bites.
async fn get_tls_status(State(state): State<AdminState>) -> Response<Body> {
    let acme = state
        .acme_status
        .read()
        .map(|status| status.clone())
        .unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let certificates = state
        .tls_resolver
        .as_ref()
        .map(|resolver| {
            resolver
                .domain_expiry()
                .into_iter()
                .map(|(domain, expires)| TlsCertStatusPayload {
                    domain,
                    expires_epoch_s: expires,
                    expires_in_days: (expires - now) / 86_400,
                })
                .collect()
        })
        .unwrap_or_default();

    json_response(StatusCode::OK, &TlsStatusPayload { acme, certificates })
}

async fn get_cache_status(
    State(state): State<AdminState>,
    Query(query): Query<CacheQuery>,
) -> Response<Body> {
    let snapshot = state.active_config.load();
    let mut routes = Vec::new();
    for idx in 0..snapshot.route_count() {
        let Some(route) = snapshot.route(idx) else {
            continue;
        };
        let Some(cache) = &route.cache else {
            continue;
        };
        if query
            .route
            .as_deref()
            .is_some_and(|name| name != route.name.as_ref())
        {
            continue;
        }
        routes.push(CacheRoutePayload {
            route: route.name.to_string(),
            entries: cache.store.entries(),
            bytes: cache.store.bytes(),
            hits: cache.store.hits(),
            misses: cache.store.misses(),
            coalesced: cache.store.coalesced(),
            evictions: cache.store.evictions(),
        });
    }
    json_response(StatusCode::OK, &routes)
}

async fn purge_cache(
    State(state): State<AdminState>,
    Query(query): Query<CacheQuery>,
) -> Response<Body> {
    let snapshot = state.active_config.load();
    let mut purged = 0usize;
    let mut touched = 0usize;
    for idx in 0..snapshot.route_count() {
        let Some(route) = snapshot.route(idx) else {
            continue;
        };
        let Some(cache) = &route.cache else {
            continue;
        };
        if query
            .route
            .as_deref()
            .is_some_and(|name| name != route.name.as_ref())
        {
            continue;
        }
        purged += cache.store.purge();
        touched += 1;
    }

    if touched == 0 && query.route.is_some() {
        return text_response(StatusCode::NOT_FOUND, b"no_such_cached_route\n".to_vec());
    }
    text_response(
        StatusCode::OK,
        format!("purged {purged} entries from {touched} route(s)\n").into_bytes(),
    )
}

async fn get_route_health(
    State(state): State<AdminState>,
    Query(query): Query<RouteHealthQuery>,
) -> Response<Body> {
    let timeout_ms = health_timeout_ms(query.timeout_ms);
    let config = match state.config_admin.read_parsed_config() {
        Ok(config) => config,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_config: {err:#}\n"),
            );
        }
    };

    let verdicts = probe_verdicts(&state.active_config);
    let payload = render_route_health_payload(config, timeout_ms, &verdicts).await;
    json_response(StatusCode::OK, &payload)
}

async fn post_route_health(
    State(state): State<AdminState>,
    Query(query): Query<RouteHealthQuery>,
    body: Body,
) -> Response<Body> {
    let timeout_ms = health_timeout_ms(query.timeout_ms);
    let body = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(body) => body,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                );
            }
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    if body.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"request_body_is_empty\n".to_vec());
    }

    let text = match std::str::from_utf8(&body) {
        Ok(content) => content,
        Err(_) => {
            return text_response(StatusCode::BAD_REQUEST, b"invalid_utf8_body\n".to_vec());
        }
    };

    let config = match PrxConfig::from_toml_str(text) {
        Ok(config) => config,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_config: {err:#}\n"),
            );
        }
    };

    let verdicts = probe_verdicts(&state.active_config);
    let payload = render_route_health_payload(config, timeout_ms, &verdicts).await;
    json_response(StatusCode::OK, &payload)
}

/// `GET /web/services/status` — live per-upstream state from the running config.
async fn get_service_status(State(state): State<AdminState>) -> Response<Body> {
    let snapshot = state.active_config.load();

    let mut services = Vec::with_capacity(snapshot.service_count());
    for idx in 0..snapshot.service_count() {
        let Some(service) = snapshot.service(idx) else {
            continue;
        };

        let shares = service.selection_share();
        let upstreams = service
            .upstreams
            .iter()
            .enumerate()
            .map(|(upstream_idx, upstream)| UpstreamStatusEntry {
                addr: upstream.addr.clone(),
                enabled: upstream.enabled,
                weight: upstream.weight,
                share: shares.get(upstream_idx).copied().unwrap_or(0.0),
                available: upstream.enabled && !upstream.is_circuit_open(),
                circuit_open: upstream.is_circuit_open(),
                circuit_reopens_in_ms: upstream.circuit_reopens_in_ms(),
                consecutive_failures: upstream.consecutive_failures(),
                probe_healthy: upstream.is_probe_healthy(),
                last_probe_ms_ago: (upstream.last_probe_ms() > 0)
                    .then(|| now_epoch_ms().saturating_sub(upstream.last_probe_ms())),
                inflight: upstream.inflight(),
                ewma_us: upstream.ewma_us(),
            })
            .collect();

        services.push(ServiceStatusEntry {
            name: service.name.clone(),
            lb: lb_to_string(service.lb.clone()).to_string(),
            health_check_enabled: service.health_check.enabled,
            circuit_breaker_enabled: service.circuit_breaker.is_enabled(),
            upstreams,
        });
    }

    json_response(
        StatusCode::OK,
        &ServiceStatusPayload {
            checked_at_epoch_ms: now_epoch_ms(),
            services,
        },
    )
}

/// `POST /web/upstreams/test` — opens a connection to one upstream and says
/// what happened.
///
/// The same TCP check the route health endpoint falls back to, addressable on
/// its own so "test this upstream" does not mean "probe the whole config".
async fn post_upstream_test(State(_state): State<AdminState>, body: Body) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    let request = match serde_json::from_slice::<UpstreamTestRequest>(&bytes) {
        Ok(request) => request,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_request_body: {err:#}\n"),
            );
        }
    };

    if request.addr.trim().is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"addr_is_empty\n".to_vec());
    }

    let timeout_ms = health_timeout_ms(request.timeout_ms);
    let result = check_upstream_health(request.addr, timeout_ms).await;
    json_response(StatusCode::OK, &result)
}

/// `POST /web/routes/test` — runs one imaginary request through the live route
/// index.
///
/// It calls the same `select` the proxy calls, on the same snapshot, so the
/// answer cannot drift from what traffic actually does; a tester that
/// re-implements the rules is a tester that lies the day the rules change.
async fn post_route_test(State(state): State<AdminState>, body: Body) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    let request = if bytes.is_empty() {
        RouteTestRequest {
            method: default_test_method(),
            host: String::new(),
            path: default_test_path(),
        }
    } else {
        match serde_json::from_slice::<RouteTestRequest>(&bytes) {
            Ok(request) => request,
            Err(err) => {
                return text_response(
                    StatusCode::BAD_REQUEST,
                    format!("invalid_request_body: {err:#}\n"),
                );
            }
        }
    };

    if !request.path.starts_with('/') {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"path_must_start_with_slash\n".to_vec(),
        );
    }

    let snapshot = state.active_config.load();
    json_response(StatusCode::OK, &run_route_test(&snapshot, &request))
}

fn run_route_test(config: &RuntimeConfig, request: &RouteTestRequest) -> RouteTestResponse {
    let normalized_host = crate::runtime::normalize_host(&request.host).into_owned();
    let outcome = config.select(&request.host, &request.path, Some(&request.method));

    let echo = RouteTestEcho {
        method: request.method.to_ascii_uppercase(),
        host: request.host.clone(),
        normalized_host: normalized_host.clone(),
        path: request.path.clone(),
    };

    let idx = match outcome {
        RouteMatch::Matched(idx) => idx,
        RouteMatch::MethodNotAllowed => {
            return RouteTestResponse {
                outcome: "method_not_allowed",
                request: echo,
                route: None,
                service: None,
            };
        }
        RouteMatch::NotFound => {
            return RouteTestResponse {
                outcome: "not_found",
                request: echo,
                route: None,
                service: None,
            };
        }
    };

    let Some(route) = config.route(idx) else {
        return RouteTestResponse {
            outcome: "not_found",
            request: echo,
            route: None,
            service: None,
        };
    };

    let matched_by = match_tier(route, &normalized_host, &request.path);
    let route_payload = RouteTestRoutePayload {
        index: idx,
        name: route.name.to_string(),
        host: route.host.clone().unwrap_or_default(),
        path_prefix: route.path_prefix.clone(),
        methods: crate::router::method_names(route.methods),
        is_default: route.is_default,
        enabled: route.enabled,
        matched_by,
    };

    let service = config.service(route.service_idx).map(|service| {
        // The same key the proxy would hash for a hash-balanced service.
        let hash_seed = crate::runtime::hash_key(&[&normalized_host, &request.path]);
        let picked = service.peek_upstream(hash_seed);
        let deterministic = picked.is_some();

        RouteTestServicePayload {
            name: service.name.clone(),
            lb: lb_to_string(service.lb.clone()).to_string(),
            selection: RouteTestSelection {
                deterministic,
                would_pick: picked.map(|(_, upstream)| upstream.addr.clone()),
                note: match &service.lb {
                    LbStrategy::RoundRobin => {
                        "Round robin: the next request takes the following upstream in the ring."
                    }
                    LbStrategy::Hash => {
                        "Hashed on host and path, so the same request always lands on the same \
                         upstream while the pool is unchanged."
                    }
                    LbStrategy::Random => "Random: any available upstream can take it.",
                    LbStrategy::LeastConn => {
                        "Least connections, sampled two at a time — the pick depends on live \
                         in-flight counts."
                    }
                    LbStrategy::P2cEwma => {
                        "Power of two choices on latency — the pick depends on live EWMA values."
                    }
                }
                .to_string(),
            },
            upstreams: service
                .upstreams
                .iter()
                .map(|upstream| RouteTestUpstream {
                    addr: upstream.addr.clone(),
                    weight: upstream.weight,
                    available: !upstream.is_circuit_open() && upstream.is_probe_healthy(),
                    circuit_open: upstream.is_circuit_open(),
                    probe_healthy: upstream.is_probe_healthy(),
                    inflight: upstream.inflight(),
                    ewma_us: upstream.ewma_us(),
                })
                .collect(),
        }
    });

    RouteTestResponse {
        outcome: "matched",
        request: echo,
        route: Some(route_payload),
        service,
    }
}

/// Which rule won. A route that covers neither the host nor the path only
/// caught the request because it is the default.
fn match_tier(
    route: &crate::runtime::RouteRuntime,
    normalized_host: &str,
    path: &str,
) -> &'static str {
    let host_tier = match route.host.as_deref() {
        None | Some("") => Some("any_host"),
        Some(pattern) => match pattern.strip_prefix("*.") {
            Some(suffix) => (normalized_host == suffix
                || normalized_host.ends_with(&format!(".{suffix}")))
            .then_some("wildcard_host"),
            None => (normalized_host == pattern).then_some("exact_host"),
        },
    };

    match host_tier {
        Some(tier) if path.starts_with(&route.path_prefix) => tier,
        _ => "default_route",
    }
}

fn lb_to_string(lb: LbStrategy) -> &'static str {
    match lb {
        LbStrategy::RoundRobin => "round_robin",
        LbStrategy::Random => "random",
        LbStrategy::Hash => "hash",
        LbStrategy::LeastConn => "least_conn",
        LbStrategy::P2cEwma => "p2c_ewma",
    }
}

fn bytes_response(
    status: StatusCode,
    content_type: &str,
    cache_control: &str,
    body: Vec<u8>,
) -> Response<Body> {
    let body_len = body.len();
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;

    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_str(cache_control)
            .unwrap_or_else(|_| HeaderValue::from_static("no-store")),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&body_len.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response
}

fn text_response(status: StatusCode, body: impl Into<Vec<u8>>) -> Response<Body> {
    bytes_response(status, "text/plain; charset=utf-8", "no-store", body.into())
}

fn json_response(status: StatusCode, payload: &impl Serialize) -> Response<Body> {
    match serde_json::to_vec(payload) {
        Ok(bytes) => bytes_response(status, "application/json; charset=utf-8", "no-store", bytes),
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_encode_json: {err:#}\n"),
        ),
    }
}

fn content_type_for(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".map") {
        "application/json; charset=utf-8"
    } else if path.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

fn static_response(path: &str, body: Vec<u8>) -> Response<Body> {
    let cache_control = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    bytes_response(StatusCode::OK, content_type_for(path), cache_control, body)
}

fn fallback_index() -> Response<Body> {
    match WEBUI_DIST.get_file(WEBUI_INDEX_PATH) {
        Some(file) => static_response(WEBUI_INDEX_PATH, file.contents().to_vec()),
        None => text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            b"webui_not_embedded\n".to_vec(),
        ),
    }
}

fn handle_webui_get(path: &str) -> Response<Body> {
    let normalized = {
        let trimmed = path.trim_start_matches('/');
        if trimmed.is_empty() {
            WEBUI_INDEX_PATH
        } else {
            trimmed
        }
    };

    if let Some(file) = WEBUI_DIST.get_file(normalized) {
        return static_response(normalized, file.contents().to_vec());
    }

    // A miss inside one of the bundled asset directories is a real 404: handing
    // back HTML for a missing script only turns it into a syntax error further
    // down the line. Everything else belongs to the client-side router, so it
    // gets index.html — including a path like /routes/api.example.com, which a
    // "does it contain a dot" test would have mistaken for a file request.
    if first_segment(normalized).is_some_and(is_asset_dir) {
        return text_response(StatusCode::NOT_FOUND, b"not_found\n".to_vec());
    }

    fallback_index()
}

fn first_segment(path: &str) -> Option<&str> {
    path.split('/').next().filter(|segment| !segment.is_empty())
}

/// True for a top-level directory that exists in the embedded UI build.
fn is_asset_dir(segment: &str) -> bool {
    WEBUI_DIST
        .dirs()
        .any(|dir| dir.path().to_str() == Some(segment))
}

/// An identity for the config file as it is on disk right now.
///
/// Two tabs that both loaded the same file get the same value, and anything
/// that rewrites the file changes it, which is all `If-Match` needs. It is not
/// a checksum anyone should trust against tampering.
fn config_etag(bytes: &[u8]) -> String {
    use std::hash::Hasher;

    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(bytes);
    format!("\"{:x}-{:x}\"", bytes.len(), hasher.finish())
}

fn with_etag(mut response: Response<Body>, etag: &str) -> Response<Body> {
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

async fn get_config(
    State(state): State<AdminState>,
    Query(query): Query<ConfigQuery>,
) -> Response<Body> {
    // Both shapes describe the same file, so both carry the same ETag: the
    // editor can load the text and still know what a JSON reader is looking at.
    let text = match state.config_admin.read_config_text() {
        Ok(text) => text,
        Err(err) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_config: {err:#}\n"),
            );
        }
    };
    let etag = config_etag(text.as_bytes());

    if query
        .format
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        return match PrxConfig::from_toml_str(&text) {
            Ok(config) => with_etag(
                json_response(StatusCode::OK, &AdminConfigPayload::from(config)),
                &etag,
            ),
            Err(err) => text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_config: {err:#}\n"),
            ),
        };
    }

    with_etag(text_response(StatusCode::OK, text.into_bytes()), &etag)
}

/// The body of a config request, or the response to send instead.
async fn read_config_body(body: Body) -> Result<String, Response<Body>> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return Err(text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                ));
            }
            return Err(text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            ));
        }
    };

    if bytes.is_empty() {
        return Err(text_response(
            StatusCode::BAD_REQUEST,
            b"request_body_is_empty\n".to_vec(),
        ));
    }

    match std::str::from_utf8(&bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) => Err(text_response(
            StatusCode::BAD_REQUEST,
            b"invalid_utf8_body\n".to_vec(),
        )),
    }
}

/// `POST /web/config/validate` (T203): every problem in one pass, each with the
/// line it is on, and nothing written.
#[derive(Debug, Serialize)]
struct ValidateResponse {
    #[serde(flatten)]
    report: ValidationReport,
    /// The config as the API would report it, when it is valid — so the editor
    /// can say what changed without parsing TOML in the browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<AdminConfigPayload>,
    /// The ETag of the file on disk at the moment of validation. A draft whose
    /// base no longer matches this was overtaken by someone else.
    #[serde(skip_serializing_if = "Option::is_none")]
    current_etag: Option<String>,
}

async fn post_config_validate(State(state): State<AdminState>, body: Body) -> Response<Body> {
    let text = match read_config_body(body).await {
        Ok(text) => text,
        Err(response) => return response,
    };

    let (report, config) = validate::validate_text(&text);
    let current_etag = state
        .config_admin
        .read_config_text()
        .ok()
        .map(|current| config_etag(current.as_bytes()));

    json_response(
        StatusCode::OK,
        &ValidateResponse {
            report,
            config: config.map(AdminConfigPayload::from),
            current_etag,
        },
    )
}

/// What a client gets back when its `If-Match` no longer matches the file:
/// enough to show the three sides without a second request.
#[derive(Debug, Serialize)]
struct ConfigConflict {
    error: &'static str,
    /// What the client believed it was editing.
    expected_etag: String,
    /// What is actually on disk.
    current_etag: String,
    current_toml: String,
}

async fn put_config(
    State(state): State<AdminState>,
    Query(query): Query<ConfigQuery>,
    headers: header::HeaderMap,
    body: Body,
) -> Response<Body> {
    let text = match read_config_body(body).await {
        Ok(text) => text,
        Err(response) => return response,
    };

    let current = state.config_admin.read_config_text().unwrap_or_default();
    let current_etag = config_etag(current.as_bytes());

    // Optimistic concurrency (T204): a client that says which version it edited
    // is told when someone else got there first, instead of silently winning.
    if let Some(expected) = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "*")
        && expected != current_etag
    {
        stats::record_event(
            EventLevel::Warn,
            "config_apply",
            "Config apply rejected: the file changed since it was loaded",
        );
        return json_response(
            StatusCode::CONFLICT,
            &ConfigConflict {
                error: "config_changed",
                expected_etag: expected.to_string(),
                current_etag,
                current_toml: current,
            },
        );
    }

    let (report, _) = validate::validate_text(&text);
    if query_flag(query.dry_run.as_ref()) {
        return json_response(
            StatusCode::OK,
            &ValidateResponse {
                report,
                config: None,
                current_etag: Some(current_etag),
            },
        );
    }

    if !report.valid {
        let first = report
            .errors
            .first()
            .map(|error| format!("line {}: {} ({})", error.line, error.message, error.code))
            .unwrap_or_else(|| "config is not valid".to_string());
        return text_response(
            StatusCode::BAD_REQUEST,
            format!("invalid_config: {first}\n"),
        );
    }

    // Applying a config is the single most consequential thing this API does,
    // so it goes on the dashboard's event strip either way round (T306).
    match state
        .config_admin
        .apply_config_text(&text, &state.active_config)
    {
        Ok(()) => {
            stats::record_event(
                EventLevel::Info,
                "config_apply",
                "Config applied from the admin API",
            );
            with_etag(
                text_response(StatusCode::OK, b"config_applied\n".to_vec()),
                &config_etag(text.as_bytes()),
            )
        }
        Err(err) => {
            stats::record_event(
                EventLevel::Error,
                "config_apply",
                format!("Config apply failed: {err}"),
            );
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_apply_config: {err:#}\n"),
            )
        }
    }
}

async fn get_webui_root() -> Response<Body> {
    handle_webui_get("")
}

async fn get_webui_path(AxumPath(path): AxumPath<String>) -> Response<Body> {
    handle_webui_get(path.as_str())
}

// ==================== Service CRUD Handlers ====================

async fn list_services(State(state): State<AdminState>) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            let services: Vec<AdminServicePayload> = config
                .services
                .iter()
                .map(AdminServicePayload::from_config)
                .collect();
            json_response(StatusCode::OK, &services)
        }
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_read_config: {err:#}\n"),
        ),
    }
}

async fn get_service(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            if let Some(service) = config.services.iter().find(|s| s.name == name) {
                let service_payload = AdminServicePayload::from_config(service);
                json_response(StatusCode::OK, &service_payload)
            } else {
                text_response(StatusCode::NOT_FOUND, b"service_not_found\n".to_vec())
            }
        }
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_read_config: {err:#}\n"),
        ),
    }
}

async fn create_service(State(state): State<AdminState>, body: Body) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                );
            }
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    if bytes.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"request_body_is_empty\n".to_vec());
    }

    let payload = match serde_json::from_slice::<ServiceRequestPayload>(&bytes) {
        Ok(p) => p,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_request_body: {err:#}\n"),
            );
        }
    };

    if payload.name.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_name_cannot_be_empty\n".to_vec(),
        );
    }

    if payload.upstreams.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_must_have_at_least_one_upstream\n".to_vec(),
        );
    }

    // Validate upstreams
    for upstream in &payload.upstreams {
        if upstream.addr.trim().is_empty() {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"upstream_addr_cannot_be_empty\n".to_vec(),
            );
        }
    }

    // Validate circuit breaker if provided
    if let Some(cb) = &payload.circuit_breaker
        && cb.enabled.unwrap_or(false)
    {
        if cb.consecutive_failures.unwrap_or(0) == 0 {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"circuit_breaker_consecutive_failures_must_be_gt_0_when_enabled\n".to_vec(),
            );
        }
        if cb.open_ms.unwrap_or(0) == 0 {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"circuit_breaker_open_ms_must_be_gt_0_when_enabled\n".to_vec(),
            );
        }
    }

    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            // Check for duplicate service name
            if config.services.iter().any(|s| s.name == payload.name) {
                return Err(anyhow::anyhow!("service '{}' already exists", payload.name));
            }

            let defaults = ServiceConfig::default();
            let service = crate::config::ServiceConfig {
                name: payload.name.clone(),
                lb: payload
                    .lb
                    .as_deref()
                    .map(|s| s.parse().unwrap_or_default())
                    .unwrap_or_default(),
                upstream_h2: payload.upstream_h2.unwrap_or(defaults.upstream_h2),
                max_retries: payload.max_retries.unwrap_or(0),
                retry_backoff_ms: payload.retry_backoff_ms.unwrap_or(0),
                retry_budget_ratio: payload
                    .retry_budget_ratio
                    .unwrap_or(defaults.retry_budget_ratio),
                retry_budget_min_per_window: payload
                    .retry_budget_min_per_window
                    .unwrap_or(defaults.retry_budget_min_per_window),
                retry_budget_window_ms: payload
                    .retry_budget_window_ms
                    .unwrap_or(defaults.retry_budget_window_ms),
                retry_idempotent_only: payload
                    .retry_idempotent_only
                    .unwrap_or(defaults.retry_idempotent_only),
                request_timeout_ms: payload
                    .request_timeout_ms
                    .unwrap_or(defaults.request_timeout_ms),
                health_check: payload.health_check.unwrap_or(defaults.health_check),
                sticky: payload.sticky.unwrap_or(defaults.sticky),
                circuit_breaker: payload
                    .circuit_breaker
                    .map(|cb| crate::config::CircuitBreakerConfig {
                        enabled: cb.enabled.unwrap_or(false),
                        consecutive_failures: cb.consecutive_failures.unwrap_or_default(),
                        open_ms: cb.open_ms.unwrap_or_default(),
                    })
                    .unwrap_or_default(),
                upstreams: payload
                    .upstreams
                    .into_iter()
                    .map(|u| crate::config::UpstreamConfig {
                        addr: u.addr,
                        enabled: u.enabled.unwrap_or(true),
                        tls: u.tls.unwrap_or(false),
                        sni: u.sni,
                        weight: u.weight.unwrap_or(1),
                        verify_cert: u.verify_cert,
                        verify_hostname: u.verify_hostname,
                        connect_timeout_ms: u.connect_timeout_ms,
                        total_connect_timeout_ms: u.total_connect_timeout_ms,
                        read_timeout_ms: u.read_timeout_ms,
                        write_timeout_ms: u.write_timeout_ms,
                        idle_timeout_ms: u.idle_timeout_ms,
                    })
                    .collect(),
            };

            config.services.push(service);
            Ok(())
        }) {
        Ok(_) => {
            let target = payload.name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Service '{}' was created", target),
                target,
            );
            text_response(StatusCode::CREATED, b"service_created\n".to_vec())
        }
        Err(err) => {
            if err.to_string().contains("already exists") {
                text_response(StatusCode::CONFLICT, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

async fn update_service(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
    body: Body,
) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                );
            }
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    if bytes.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"request_body_is_empty\n".to_vec());
    }

    let payload = match serde_json::from_slice::<ServiceRequestPayload>(&bytes) {
        Ok(p) => p,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_request_body: {err:#}\n"),
            );
        }
    };

    if payload.name != name {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_name_in_path_must_match_name_in_body\n".to_vec(),
        );
    }

    if payload.upstreams.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_must_have_at_least_one_upstream\n".to_vec(),
        );
    }

    // Validate upstreams
    for upstream in &payload.upstreams {
        if upstream.addr.trim().is_empty() {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"upstream_addr_cannot_be_empty\n".to_vec(),
            );
        }
    }

    // Validate circuit breaker if provided
    if let Some(cb) = &payload.circuit_breaker
        && cb.enabled.unwrap_or(false)
    {
        if cb.consecutive_failures.unwrap_or(0) == 0 {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"circuit_breaker_consecutive_failures_must_be_gt_0_when_enabled\n".to_vec(),
            );
        }
        if cb.open_ms.unwrap_or(0) == 0 {
            return text_response(
                StatusCode::BAD_REQUEST,
                b"circuit_breaker_open_ms_must_be_gt_0_when_enabled\n".to_vec(),
            );
        }
    }

    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            let index = config
                .services
                .iter()
                .position(|s| s.name == name)
                .ok_or_else(|| anyhow::anyhow!("service '{}' not found", name))?;

            let service = crate::config::ServiceConfig {
                name: payload.name.clone(),
                lb: payload
                    .lb
                    .as_deref()
                    .map(|s| s.parse().unwrap_or_default())
                    .unwrap_or_else(|| config.services[index].lb.clone()),
                // A block the request does not mention keeps whatever the file
                // said. This used to fall through to `..Default::default()`,
                // which quietly reset health checks, session affinity and retry
                // budgets every time anyone renamed a service in the UI.
                upstream_h2: payload
                    .upstream_h2
                    .unwrap_or(config.services[index].upstream_h2),
                max_retries: payload
                    .max_retries
                    .unwrap_or(config.services[index].max_retries),
                retry_backoff_ms: payload
                    .retry_backoff_ms
                    .unwrap_or(config.services[index].retry_backoff_ms),
                retry_budget_ratio: payload
                    .retry_budget_ratio
                    .unwrap_or(config.services[index].retry_budget_ratio),
                retry_budget_min_per_window: payload
                    .retry_budget_min_per_window
                    .unwrap_or(config.services[index].retry_budget_min_per_window),
                retry_budget_window_ms: payload
                    .retry_budget_window_ms
                    .unwrap_or(config.services[index].retry_budget_window_ms),
                retry_idempotent_only: payload
                    .retry_idempotent_only
                    .unwrap_or(config.services[index].retry_idempotent_only),
                request_timeout_ms: payload
                    .request_timeout_ms
                    .unwrap_or(config.services[index].request_timeout_ms),
                health_check: payload
                    .health_check
                    .unwrap_or_else(|| config.services[index].health_check.clone()),
                sticky: payload
                    .sticky
                    .unwrap_or_else(|| config.services[index].sticky.clone()),
                circuit_breaker: payload
                    .circuit_breaker
                    .map(|cb| crate::config::CircuitBreakerConfig {
                        enabled: cb
                            .enabled
                            .unwrap_or(config.services[index].circuit_breaker.enabled),
                        consecutive_failures: cb
                            .consecutive_failures
                            .unwrap_or(config.services[index].circuit_breaker.consecutive_failures),
                        open_ms: cb
                            .open_ms
                            .unwrap_or(config.services[index].circuit_breaker.open_ms),
                    })
                    .unwrap_or_else(|| config.services[index].circuit_breaker.clone()),
                upstreams: payload
                    .upstreams
                    .into_iter()
                    .map(|u| crate::config::UpstreamConfig {
                        addr: u.addr,
                        enabled: u.enabled.unwrap_or(true),
                        tls: u.tls.unwrap_or(false),
                        sni: u.sni,
                        weight: u.weight.unwrap_or(1),
                        verify_cert: u.verify_cert,
                        verify_hostname: u.verify_hostname,
                        connect_timeout_ms: u.connect_timeout_ms,
                        total_connect_timeout_ms: u.total_connect_timeout_ms,
                        read_timeout_ms: u.read_timeout_ms,
                        write_timeout_ms: u.write_timeout_ms,
                        idle_timeout_ms: u.idle_timeout_ms,
                    })
                    .collect(),
            };

            config.services[index] = service;
            Ok(())
        }) {
        Ok(_) => {
            let target = name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Service '{}' was updated", target),
                target,
            );
            text_response(StatusCode::OK, b"service_updated\n".to_vec())
        }
        Err(err) => {
            if err.to_string().contains("not found") {
                text_response(StatusCode::NOT_FOUND, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

async fn delete_service(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
) -> Response<Body> {
    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            let index = config
                .services
                .iter()
                .position(|s| s.name == name)
                .ok_or_else(|| anyhow::anyhow!("service '{}' not found", name))?;

            // Naming them matters: "something still points at this" is not
            // an answer anyone can act on.
            let referenced: Vec<&str> = config
                .routes
                .iter()
                .filter(|r| r.service == name)
                .map(|r| r.name.as_str())
                .collect();
            if !referenced.is_empty() {
                return Err(anyhow::anyhow!(
                    "service '{}' is referenced by {} route(s): {}",
                    name,
                    referenced.len(),
                    referenced.join(", ")
                ));
            }

            config.services.remove(index);
            Ok(())
        }) {
        Ok(_) => {
            let target = name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Service '{}' was deleted", target),
                target,
            );
            text_response(StatusCode::OK, b"service_deleted\n".to_vec())
        }
        Err(err) => {
            if err.to_string().contains("not found") {
                text_response(StatusCode::NOT_FOUND, format!("{err:#}\n"))
            } else if err.to_string().contains("referenced") {
                text_response(StatusCode::CONFLICT, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

// ==================== Route CRUD Handlers ====================

async fn list_routes(State(state): State<AdminState>) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            let routes: Vec<AdminRoutePayload> = config
                .routes
                .iter()
                .map(AdminRoutePayload::from_config)
                .collect();
            json_response(StatusCode::OK, &routes)
        }
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_read_config: {err:#}\n"),
        ),
    }
}

async fn get_route(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            if let Some(route) = config.routes.iter().find(|r| r.name == name) {
                json_response(StatusCode::OK, &AdminRoutePayload::from_config(route))
            } else {
                text_response(StatusCode::NOT_FOUND, b"route_not_found\n".to_vec())
            }
        }
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_read_config: {err:#}\n"),
        ),
    }
}

async fn create_route(State(state): State<AdminState>, body: Body) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                );
            }
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    if bytes.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"request_body_is_empty\n".to_vec());
    }

    let payload = match serde_json::from_slice::<RouteRequestPayload>(&bytes) {
        Ok(p) => p,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_request_body: {err:#}\n"),
            );
        }
    };

    if payload.name.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_name_cannot_be_empty\n".to_vec(),
        );
    }

    if payload.service.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_cannot_be_empty\n".to_vec(),
        );
    }

    let path_prefix = payload.path_prefix.as_deref().unwrap_or("/");
    if path_prefix.is_empty() || !path_prefix.starts_with('/') {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_path_prefix_must_be_non_empty_and_start_with_slash\n".to_vec(),
        );
    }

    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            // Check for duplicate route name
            if config.routes.iter().any(|r| r.name == payload.name) {
                return Err(anyhow::anyhow!("route '{}' already exists", payload.name));
            }

            // Check if service exists
            if !config.services.iter().any(|s| s.name == payload.service) {
                return Err(anyhow::anyhow!("service '{}' not found", payload.service));
            }

            // Check for duplicate default route
            if payload.is_default.unwrap_or(false) && config.routes.iter().any(|r| r.is_default) {
                return Err(anyhow::anyhow!("only one route can be marked as default"));
            }

            let defaults = crate::config::RouteConfig::default();
            let route = crate::config::RouteConfig {
                name: payload.name.clone(),
                service: payload.service.clone(),
                host: payload.host,
                path_prefix: payload.path_prefix.unwrap_or_else(|| "/".to_string()),
                methods: payload.methods.unwrap_or_default(),
                is_default: payload.is_default.unwrap_or(false),
                enabled: payload.enabled.unwrap_or(true),
                request_headers: payload.request_headers.unwrap_or(defaults.request_headers),
                response_headers: payload
                    .response_headers
                    .unwrap_or(defaults.response_headers),
                rate_limit: payload.rate_limit.unwrap_or(defaults.rate_limit),
                concurrency_limit: payload
                    .concurrency_limit
                    .unwrap_or(defaults.concurrency_limit),
                cache: payload.cache.unwrap_or(defaults.cache),
            };

            config.routes.push(route);
            Ok(())
        }) {
        Ok(_) => {
            let target = payload.name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Route '{}' was created", target),
                target,
            );
            text_response(StatusCode::CREATED, b"route_created\n".to_vec())
        }
        Err(err) => {
            let err_str = err.to_string();
            if err_str.contains("already exists") {
                text_response(StatusCode::CONFLICT, format!("{err:#}\n"))
            } else if err_str.contains("not found") {
                text_response(StatusCode::BAD_REQUEST, format!("{err:#}\n"))
            } else if err_str.contains("only one route can be marked as default") {
                text_response(StatusCode::CONFLICT, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

async fn update_route(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
    body: Body,
) -> Response<Body> {
    let bytes = match body::to_bytes(body, MAX_ADMIN_CONFIG_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.to_string().to_ascii_lowercase().contains("limit") {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    b"request_body_too_large\n".to_vec(),
                );
            }
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_request_body: {err:#}\n"),
            );
        }
    };

    if bytes.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"request_body_is_empty\n".to_vec());
    }

    let payload = match serde_json::from_slice::<RouteRequestPayload>(&bytes) {
        Ok(p) => p,
        Err(err) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("invalid_request_body: {err:#}\n"),
            );
        }
    };

    if payload.name != name {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_name_in_path_must_match_name_in_body\n".to_vec(),
        );
    }

    if payload.service.is_empty() {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"service_cannot_be_empty\n".to_vec(),
        );
    }

    let path_prefix = payload.path_prefix.as_deref().unwrap_or("/");
    if path_prefix.is_empty() || !path_prefix.starts_with('/') {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_path_prefix_must_be_non_empty_and_start_with_slash\n".to_vec(),
        );
    }

    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            let index = config
                .routes
                .iter()
                .position(|r| r.name == name)
                .ok_or_else(|| anyhow::anyhow!("route '{}' not found", name))?;

            // Check if service exists
            if !config.services.iter().any(|s| s.name == payload.service) {
                return Err(anyhow::anyhow!("service '{}' not found", payload.service));
            }

            // Check for duplicate default route
            let current_default = config.routes[index].is_default;
            let new_default = payload.is_default.unwrap_or(current_default);
            if new_default
                && !current_default
                && config.routes.iter().any(|r| r.is_default && r.name != name)
            {
                return Err(anyhow::anyhow!("only one route can be marked as default"));
            }

            let route = crate::config::RouteConfig {
                name: payload.name.clone(),
                service: payload.service.clone(),
                host: payload.host,
                path_prefix: payload.path_prefix.unwrap_or_else(|| "/".to_string()),
                methods: payload
                    .methods
                    .unwrap_or_else(|| config.routes[index].methods.clone()),
                is_default: payload
                    .is_default
                    .unwrap_or(config.routes[index].is_default),
                enabled: payload.enabled.unwrap_or(config.routes[index].enabled),
                // A block the request does not mention keeps whatever the file
                // said, so an older client cannot wipe settings it has no field
                // for.
                request_headers: payload
                    .request_headers
                    .unwrap_or_else(|| config.routes[index].request_headers.clone()),
                response_headers: payload
                    .response_headers
                    .unwrap_or_else(|| config.routes[index].response_headers.clone()),
                rate_limit: payload
                    .rate_limit
                    .unwrap_or_else(|| config.routes[index].rate_limit.clone()),
                concurrency_limit: payload
                    .concurrency_limit
                    .unwrap_or_else(|| config.routes[index].concurrency_limit.clone()),
                cache: payload
                    .cache
                    .unwrap_or_else(|| config.routes[index].cache.clone()),
            };

            config.routes[index] = route;
            Ok(())
        }) {
        Ok(_) => {
            let target = name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Route '{}' was updated", target),
                target,
            );
            text_response(StatusCode::OK, b"route_updated\n".to_vec())
        }
        Err(err) => {
            let err_str = err.to_string();
            if err_str.contains("not found") {
                text_response(StatusCode::NOT_FOUND, format!("{err:#}\n"))
            } else if err_str.contains("only one route can be marked as default") {
                text_response(StatusCode::CONFLICT, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

async fn delete_route(
    State(state): State<AdminState>,
    AxumPath(name): AxumPath<String>,
) -> Response<Body> {
    match state
        .config_admin
        .modify_config(&state.active_config, |config| {
            let index = config
                .routes
                .iter()
                .position(|r| r.name == name)
                .ok_or_else(|| anyhow::anyhow!("route '{}' not found", name))?;

            config.routes.remove(index);
            Ok(())
        }) {
        Ok(_) => {
            let target = name.clone();
            stats::record_event_for(
                EventLevel::Info,
                "config_apply",
                format!("Route '{}' was deleted", target),
                target,
            );
            text_response(StatusCode::OK, b"route_deleted\n".to_vec())
        }
        Err(err) => {
            if err.to_string().contains("not found") {
                text_response(StatusCode::NOT_FOUND, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

// --- live stats (T207) ------------------------------------------------------
//
// The dashboard needs "what is happening now", which `/web/config` cannot
// answer and `/metrics` only answers to a Prometheus. `GET /web/stats` is the
// first read, `GET /web/stats/stream` is every read after that.

/// Keeps a stream's slot taken for exactly as long as the stream is alive.
///
/// The client cap only works if a slot comes back when the tab closes, and the
/// only thing that reliably notices a closed tab is the response body being
/// dropped — so the guard rides along inside it. There is no per-client task to
/// leak: everyone reads the same broadcast.
struct GuardedStream<S> {
    inner: S,
    _guard: stats::ClientGuard,
}

impl<S> tokio_stream::Stream for GuardedStream<S>
where
    S: tokio_stream::Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<S::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

async fn get_stats() -> Response<Body> {
    json_response(StatusCode::OK, &stats::hub().snapshot())
}

async fn get_stats_stream() -> Response<Body> {
    let Some((receiver, guard)) = stats::hub().subscribe() else {
        // Refusing is better than letting a wall of dashboards turn the control
        // plane into a fan-out service; the client falls back to polling.
        return text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "too_many_stats_clients: at most {} streams at a time\n",
                stats::MAX_STREAM_CLIENTS
            ),
        );
    };

    let ticks = tokio_stream::StreamExt::map(
        tokio_stream::wrappers::BroadcastStream::new(receiver),
        |tick| {
            let event = match tick {
                Ok(tick) => Event::default()
                    .id(tick.seq.to_string())
                    .event("tick")
                    .json_data(&*tick)
                    .unwrap_or_else(|_| Event::default().event("error").data("encode_failed")),
                // A client too slow to keep up is told it fell behind rather
                // than shown a chart with a silent hole in it.
                Err(_) => Event::default().event("lagged").data("refetch"),
            };
            Ok::<Event, Infallible>(event)
        },
    );

    // The retry hint is what makes reconnection the browser's problem instead
    // of ours; `hello` tells the page the stream is live again.
    let hello = tokio_stream::iter([Ok::<Event, Infallible>(
        Event::default()
            .event("hello")
            .retry(Duration::from_secs(2))
            .data(stats::hub().snapshot().seq.to_string()),
    )]);

    let stream = GuardedStream {
        inner: tokio_stream::StreamExt::chain(hello, ticks),
        _guard: guard,
    };

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

fn build_router(state: AdminState) -> Router {
    Router::new()
        // Config endpoints
        .route(ADMIN_CONFIG_PATH, get(get_config).put(put_config))
        .route(
            ADMIN_CONFIG_VALIDATE_PATH,
            axum::routing::post(post_config_validate),
        )
        .route(
            ADMIN_ROUTE_HEALTH_PATH,
            get(get_route_health).post(post_route_health),
        )
        .route(ADMIN_ROUTE_TEST_PATH, axum::routing::post(post_route_test))
        .route(ADMIN_SERVICE_STATUS_PATH, get(get_service_status))
        .route(
            ADMIN_UPSTREAM_TEST_PATH,
            axum::routing::post(post_upstream_test),
        )
        .route(ADMIN_STATS_PATH, get(get_stats))
        .route(ADMIN_STATS_STREAM_PATH, get(get_stats_stream))
        .route(ADMIN_CACHE_PATH, get(get_cache_status).delete(purge_cache))
        .route(ADMIN_TLS_STATUS_PATH, get(get_tls_status))
        // Service CRUD endpoints
        .route(ADMIN_SERVICES_PATH, get(list_services).post(create_service))
        .route(
            ADMIN_SERVICES_NAME_PATH,
            get(get_service).put(update_service).delete(delete_service),
        )
        // Route CRUD endpoints
        .route(ADMIN_ROUTES_PATH, get(list_routes).post(create_route))
        .route(
            ADMIN_ROUTES_NAME_PATH,
            get(get_route).put(update_route).delete(delete_route),
        )
        // WebUI
        .route("/", get(get_webui_root))
        .route("/{*path}", get(get_webui_path))
        .with_state(state)
}

pub fn bind_admin_listener(listen: &str) -> anyhow::Result<TcpListener> {
    TcpListener::bind(listen).with_context(|| format!("failed to bind admin listener on {listen}"))
}

pub struct AdminAxumService {
    name: String,
    listen: String,
    listener: Option<TcpListener>,
    state: AdminState,
}

impl AdminAxumService {
    pub fn new(
        listen: String,
        listener: TcpListener,
        config_path: PathBuf,
        active_config: Arc<ArcSwap<RuntimeConfig>>,
        acme_status: crate::acme::SharedStatus,
        tls_resolver: Option<Arc<crate::tls::CertResolver>>,
    ) -> Self {
        Self {
            name: "prx-admin-axum".to_string(),
            listen,
            listener: Some(listener),
            state: AdminState {
                config_admin: ConfigAdmin::new(config_path),
                active_config,
                acme_status,
                tls_resolver,
            },
        }
    }
}

#[async_trait]
impl Service for AdminAxumService {
    async fn start_service(
        &mut self,
        #[cfg(unix)] _fds: Option<pingora::server::ListenFds>,
        mut shutdown: pingora::server::ShutdownWatch,
        _listeners_per_fd: usize,
    ) {
        let Some(listener) = self.listener.take() else {
            error!("admin listener is unavailable; service may have been started more than once");
            return;
        };

        if let Err(err) = listener.set_nonblocking(true) {
            error!(
                error = %err,
                listen = self.listen.as_str(),
                "failed to set admin listener as nonblocking"
            );
            return;
        }

        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(err) => {
                error!(
                    error = %err,
                    listen = self.listen.as_str(),
                    "failed to convert admin listener for tokio"
                );
                return;
            }
        };

        info!(
            listen = self.listen.as_str(),
            path = ADMIN_CONFIG_PATH,
            "admin config API is enabled"
        );

        // The live-stats sampler runs on the admin runtime, not the proxy's, so
        // a dashboard can never slow a request down (T207).
        stats::spawn_sampler(self.state.active_config.clone());

        let app = build_router(self.state.clone());
        let shutdown_signal = async move {
            let _ = shutdown.changed().await;
        };

        if let Err(err) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            error!(
                error = %err,
                listen = self.listen.as_str(),
                "admin axum server stopped"
            );
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn threads(&self) -> Option<usize> {
        Some(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use tempfile::tempdir;

    use crate::runtime::RuntimeConfig;

    /// A config with one route per matching tier, so the tester can be checked
    /// against the matcher on every rule rather than only the easy one.
    const TEST_ROUTES: &str = r#"
[server]
listen = ["127.0.0.1:18080"]
health_path = "/healthz"
ready_path = "/readyz"

[[service]]
name = "api"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:19001"

[[service.upstream]]
addr = "127.0.0.1:19002"

[[route]]
name = "exact-deep"
service = "api"
host = "api.example.com"
path_prefix = "/v1/users"

[[route]]
name = "exact-shallow"
service = "api"
host = "api.example.com"
path_prefix = "/"

[[route]]
name = "wildcard"
service = "api"
host = "*.example.com"
path_prefix = "/"

[[route]]
name = "any-host"
service = "api"
path_prefix = "/internal"

[[route]]
name = "get-only"
service = "api"
host = "methods.example.com"
path_prefix = "/"
methods = ["GET"]

[[route]]
name = "parked"
service = "api"
host = "parked.example.com"
path_prefix = "/"
enabled = false

[[route]]
name = "fallback"
service = "api"
path_prefix = "/nothing-matches-this"
is_default = true
"#;

    fn test_runtime() -> RuntimeConfig {
        let config = PrxConfig::from_toml_str(TEST_ROUTES).expect("test config parses");
        config.validate().expect("test config is valid");
        RuntimeConfig::from_config(config)
    }

    fn test_request(method: &str, host: &str, path: &str) -> RouteTestRequest {
        RouteTestRequest {
            method: method.to_string(),
            host: host.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn route_tester_agrees_with_the_matcher() {
        // The point of the endpoint is that it cannot disagree with production
        // traffic, so the test asserts exactly that rather than re-deriving the
        // expected route by hand.
        let runtime = test_runtime();
        let cases = [
            ("GET", "api.example.com", "/v1/users/42"),
            ("GET", "api.example.com", "/something-else"),
            ("POST", "shop.example.com", "/cart"),
            ("GET", "example.com", "/"),
            ("GET", "other.test", "/internal/metrics"),
            ("GET", "other.test", "/elsewhere"),
            ("POST", "methods.example.com", "/"),
            ("GET", "methods.example.com", "/"),
            ("GET", "parked.example.com", "/"),
            ("GET", "api.example.com:8443", "/v1/users"),
        ];

        for (method, host, path) in cases {
            let response = run_route_test(&runtime, &test_request(method, host, path));
            match runtime.select(host, path, Some(method)) {
                RouteMatch::Matched(idx) => {
                    assert_eq!(response.outcome, "matched", "{method} {host}{path}");
                    assert_eq!(
                        response.route.expect("a matched route is reported").index,
                        idx,
                        "{method} {host}{path}"
                    );
                }
                RouteMatch::MethodNotAllowed => {
                    assert_eq!(
                        response.outcome, "method_not_allowed",
                        "{method} {host}{path}"
                    );
                    assert!(response.route.is_none());
                }
                RouteMatch::NotFound => {
                    assert_eq!(response.outcome, "not_found", "{method} {host}{path}");
                    assert!(response.route.is_none());
                }
            }
        }
    }

    #[test]
    fn route_tester_names_the_rule_that_won() {
        let runtime = test_runtime();
        let tier = |method: &str, host: &str, path: &str| {
            run_route_test(&runtime, &test_request(method, host, path))
                .route
                .expect("matched")
                .matched_by
        };

        assert_eq!(tier("GET", "api.example.com", "/v1/users/42"), "exact_host");
        assert_eq!(tier("GET", "shop.example.com", "/cart"), "wildcard_host");
        assert_eq!(tier("GET", "other.test", "/internal/metrics"), "any_host");
        // Nothing covers this host or path: only the default route caught it.
        assert_eq!(tier("GET", "other.test", "/elsewhere"), "default_route");
    }

    #[test]
    fn route_tester_reports_the_route_a_request_would_reach() {
        let runtime = test_runtime();
        let route = |method: &str, host: &str, path: &str| {
            run_route_test(&runtime, &test_request(method, host, path))
                .route
                .expect("matched")
                .name
        };

        // Longest path prefix wins inside one host...
        assert_eq!(
            route("GET", "api.example.com", "/v1/users/42"),
            "exact-deep"
        );
        assert_eq!(route("GET", "api.example.com", "/v1"), "exact-shallow");
        // ...and an exact host beats the wildcard that also covers it.
        assert_eq!(route("GET", "api.example.com", "/"), "exact-shallow");
        assert_eq!(route("GET", "shop.example.com", "/"), "wildcard");
    }

    #[test]
    fn a_disabled_route_never_matches() {
        let runtime = test_runtime();
        let response = run_route_test(&runtime, &test_request("GET", "parked.example.com", "/"));
        // The parked route is out of the index entirely, so the next rule that
        // covers the host takes the request — here the `*.example.com` route.
        assert_eq!(response.outcome, "matched");
        assert_eq!(response.route.expect("matched").name, "wildcard");
    }

    #[test]
    fn the_tester_does_not_disturb_the_load_balancer() {
        // Answering "where would this go?" must not consume a round-robin slot;
        // otherwise looking at the UI would skew live traffic.
        let runtime = test_runtime();
        let first = run_route_test(&runtime, &test_request("GET", "api.example.com", "/"))
            .service
            .expect("service")
            .selection
            .would_pick;
        let second = run_route_test(&runtime, &test_request("GET", "api.example.com", "/"))
            .service
            .expect("service")
            .selection
            .would_pick;
        assert_eq!(first, second);
        assert!(first.is_some(), "round robin has a next upstream to name");
    }

    const WEIGHTED_SERVICE: &str = r#"
[server]
listen = ["127.0.0.1:18080"]
health_path = "/healthz"
ready_path = "/readyz"

[[service]]
name = "weighted"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:19001"
weight = 3

[[service.upstream]]
addr = "127.0.0.1:19002"
weight = 1

[[route]]
name = "default"
service = "weighted"
path_prefix = "/"
is_default = true
"#;

    #[test]
    fn the_share_a_weight_buys_is_the_share_traffic_gets() {
        // The UI puts a percentage next to the weight slider. This is the test
        // that the percentage is not a story: the reported share and the
        // distribution round robin actually produces have to agree.
        let config = PrxConfig::from_toml_str(WEIGHTED_SERVICE).expect("config parses");
        let runtime = RuntimeConfig::from_config(config);
        let service = runtime.service(0).expect("service");

        let shares = service.selection_share();
        assert_eq!(shares.len(), 2);
        assert!((shares[0] - 0.75).abs() < 1e-9, "3:1 is 75% of the ring");
        assert!((shares[1] - 0.25).abs() < 1e-9);

        let mut picked = [0usize; 2];
        for _ in 0..4_000 {
            let (idx, _) = service.next_upstream(0, &[]).expect("an upstream");
            picked[idx] += 1;
        }
        let observed = [picked[0] as f64 / 4_000.0, picked[1] as f64 / 4_000.0];
        for (share, seen) in shares.iter().zip(observed.iter()) {
            assert!(
                (share - seen).abs() < 0.01,
                "reported {share} but round robin sent {seen}"
            );
        }
    }

    #[test]
    fn weight_zero_is_not_a_drain() {
        // The balancer clamps weights to at least 1, so an upstream set to
        // weight 0 quietly keeps serving. This is why draining is its own flag
        // rather than a weight trick, and this test is what keeps it that way.
        let toml = WEIGHTED_SERVICE.replace("weight = 1", "weight = 0");
        let config = PrxConfig::from_toml_str(&toml).expect("config parses");
        let runtime = RuntimeConfig::from_config(config);
        let service = runtime.service(0).expect("service");

        assert!(service.selection_share()[1] > 0.0);
    }

    #[test]
    fn a_drained_upstream_takes_no_traffic() {
        let toml = WEIGHTED_SERVICE.replace(
            "addr = \"127.0.0.1:19002\"\nweight = 1",
            "addr = \"127.0.0.1:19002\"\nweight = 1\nenabled = false",
        );
        let config = PrxConfig::from_toml_str(&toml).expect("config parses");
        let runtime = RuntimeConfig::from_config(config);
        let service = runtime.service(0).expect("service");

        let shares = service.selection_share();
        assert_eq!(
            shares[1], 0.0,
            "a drained upstream has no share of the ring"
        );
        assert!((shares[0] - 1.0).abs() < 1e-9, "the rest takes everything");

        for _ in 0..200 {
            let (idx, _) = service.next_upstream(0, &[]).expect("an upstream");
            assert_eq!(idx, 0, "traffic never reaches the drained upstream");
        }

        // It is still in the config, and still worth probing, so it can be
        // brought back on evidence rather than on hope.
        assert_eq!(service.upstreams.len(), 2);
        assert!(!service.upstreams[1].enabled);
    }

    #[test]
    fn draining_every_upstream_leaves_nothing_to_pick() {
        let toml = WEIGHTED_SERVICE
            .replace("weight = 3", "weight = 3\nenabled = false")
            .replace("weight = 1", "weight = 1\nenabled = false");
        let config = PrxConfig::from_toml_str(&toml).expect("config parses");
        let runtime = RuntimeConfig::from_config(config);
        let service = runtime.service(0).expect("service");

        assert!(service.next_upstream(0, &[]).is_none());
        assert!(!service.has_available_upstream());
    }

    #[test]
    fn service_status_reports_every_upstream() {
        let config = PrxConfig::from_toml_str(WEIGHTED_SERVICE).expect("config parses");
        let runtime = RuntimeConfig::from_config(config);
        let service = runtime.service(0).expect("service");

        let shares = service.selection_share();
        assert_eq!(service.upstreams.len(), shares.len());
        for upstream in &service.upstreams {
            // Nothing has failed yet, so the breaker is shut and nothing is due.
            assert!(!upstream.is_circuit_open());
            assert_eq!(upstream.circuit_reopens_in_ms(), None);
            assert_eq!(upstream.consecutive_failures(), 0);
        }
    }

    #[test]
    fn webui_serves_embedded_assets() {
        let response = handle_webui_get("index.html");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn webui_falls_back_to_index_for_client_routes() {
        for path in ["routes", "routes/api-v1", "services/checkout"] {
            let response = handle_webui_get(path);
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{path} should be handed to the client router"
            );
        }
    }

    #[test]
    fn webui_falls_back_for_a_route_named_after_a_host() {
        // Route names are often hostnames. The dot in one used to be read as a
        // file extension, which 404'd a link the UI hands out itself.
        let response = handle_webui_get("routes/api.example.com");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn webui_still_404s_a_missing_asset() {
        // HTML in place of a missing script would surface as a parse error in
        // the browser rather than a cache miss anyone can read.
        let response = handle_webui_get("assets/not-a-real-bundle.js");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    fn sample_config(listen: &str) -> String {
        format!(
            r#"[server]
listen = ["{listen}"]
health_path = "/healthz"
ready_path = "/readyz"

[[service]]
name = "default"
lb = "round_robin"
max_retries = 0
retry_backoff_ms = 0

[[service.upstream]]
addr = "127.0.0.1:9000"

[[route]]
name = "default"
service = "default"
path_prefix = "/"
is_default = true
"#
        )
    }

    #[test]
    fn atomic_replace_overwrites_target() {
        let dir = tempdir().expect("tempdir should be created");
        let config_path = dir.path().join("Prx.toml");
        fs::write(&config_path, "[server]\nlisten=[\"0.0.0.0:8080\"]\n").expect("seed file");

        ConfigAdmin::atomic_replace(&config_path, b"hello").expect("atomic replace should work");

        let content = fs::read_to_string(&config_path).expect("should read replaced file");
        assert_eq!(content, "hello");
    }

    #[test]
    fn apply_config_text_replaces_file_content() {
        let dir = tempdir().expect("tempdir should be created");
        let config_path = dir.path().join("Prx.toml");

        let current = sample_config("127.0.0.1:8080");
        fs::write(&config_path, &current).expect("seed config");
        let current_parsed =
            PrxConfig::from_file(&config_path).expect("seed config should be valid");
        let runtime = Arc::new(ArcSwap::from_pointee(RuntimeConfig::from_config(
            current_parsed,
        )));

        let next = sample_config("127.0.0.1:8081");
        PrxConfig::from_toml_str(&next).expect("next config should be valid");

        let admin = ConfigAdmin::new(config_path.clone());
        admin
            .apply_config_text(&next, &runtime)
            .expect("apply config should succeed");

        let content = fs::read_to_string(&config_path).expect("config should be readable");
        assert_eq!(content, next);
    }

    // ---- validate, ETag and If-Match (T203/T204, for the editor in T307) ----

    fn admin_state(config_path: &Path) -> AdminState {
        let parsed = PrxConfig::from_file(config_path).expect("seed config should be valid");
        AdminState {
            config_admin: ConfigAdmin::new(config_path.to_path_buf()),
            active_config: Arc::new(ArcSwap::from_pointee(RuntimeConfig::from_config(parsed))),
            acme_status: Arc::new(std::sync::RwLock::new(crate::acme::AcmeStatus::default())),
            tls_resolver: None,
        }
    }

    async fn response_text(response: Response<Body>) -> String {
        let bytes = body::to_bytes(response.into_body(), MAX_ADMIN_CONFIG_BODY_BYTES)
            .await
            .expect("body should be readable");
        String::from_utf8(bytes.to_vec()).expect("body should be utf-8")
    }

    fn seeded(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir should be created");
        let config_path = dir.path().join("Prx.toml");
        fs::write(&config_path, text).expect("seed config");
        (dir, config_path)
    }

    #[tokio::test]
    async fn get_config_carries_an_etag_that_follows_the_file() {
        let (_dir, config_path) = seeded(&sample_config("127.0.0.1:8080"));
        let state = admin_state(&config_path);

        let first = get_config(State(state.clone()), Query(ConfigQuery::default())).await;
        let etag = first
            .headers()
            .get(header::ETAG)
            .expect("an ETag so a draft knows what it is based on")
            .to_str()
            .expect("ascii")
            .to_string();

        // The JSON shape describes the same file, so it answers with the same
        // ETag rather than one of its own.
        let as_json = get_config(
            State(state.clone()),
            Query(ConfigQuery {
                format: Some("json".to_string()),
                dry_run: None,
            }),
        )
        .await;
        assert_eq!(
            as_json
                .headers()
                .get(header::ETAG)
                .unwrap()
                .to_str()
                .unwrap(),
            etag
        );

        fs::write(&config_path, sample_config("127.0.0.1:8081")).expect("rewrite config");
        let after = get_config(State(state), Query(ConfigQuery::default())).await;
        assert_ne!(
            after.headers().get(header::ETAG).unwrap().to_str().unwrap(),
            etag,
            "a rewritten file has to look different"
        );
    }

    #[tokio::test]
    async fn validate_reports_every_error_with_its_line() {
        let (_dir, config_path) = seeded(&sample_config("127.0.0.1:8080"));
        let state = admin_state(&config_path);

        let draft = sample_config("127.0.0.1:8080")
            .replace("service = \"default\"", "service = \"missing\"");
        let response = post_config_validate(State(state), Body::from(draft)).await;
        assert_eq!(response.status(), StatusCode::OK);

        let payload: serde_json::Value =
            serde_json::from_str(&response_text(response).await).expect("json report");
        assert_eq!(payload["valid"], false);
        assert_eq!(payload["errors"][0]["code"], "unknown_service");
        assert!(payload["errors"][0]["line"].as_u64().unwrap() > 1);
        assert!(payload["current_etag"].is_string());
        assert!(
            payload["config"].is_null(),
            "an invalid draft has no config to report"
        );
    }

    #[tokio::test]
    async fn validate_hands_back_the_parsed_config_and_its_warnings() {
        let (_dir, config_path) = seeded(&sample_config("127.0.0.1:8080"));
        let state = admin_state(&config_path);

        // A second service nothing routes to: valid, but worth saying.
        let draft = format!(
            "{}\n[[service]]\nname = \"spare\"\n\n[[service.upstream]]\naddr = \"127.0.0.1:9001\"\n",
            sample_config("127.0.0.1:8080")
        );
        let response = post_config_validate(State(state), Body::from(draft)).await;

        let payload: serde_json::Value =
            serde_json::from_str(&response_text(response).await).expect("json report");
        assert_eq!(payload["valid"], true);
        assert_eq!(payload["config"]["services"][1]["name"], "spare");
        assert_eq!(payload["warnings"][0]["code"], "unused_service");
        assert_eq!(payload["warnings"][0]["severity"], "warning");
    }

    #[tokio::test]
    async fn an_apply_that_was_overtaken_is_a_conflict_not_a_silent_win() {
        let (_dir, config_path) = seeded(&sample_config("127.0.0.1:8080"));
        let state = admin_state(&config_path);

        // What this tab loaded...
        let loaded = get_config(State(state.clone()), Query(ConfigQuery::default())).await;
        let stale_etag = loaded
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // ...and what somebody else did in the meantime.
        let theirs = sample_config("127.0.0.1:9999");
        fs::write(&config_path, &theirs).expect("someone else writes the file");

        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::IF_MATCH,
            HeaderValue::from_str(&stale_etag).unwrap(),
        );
        let mine = sample_config("127.0.0.1:8081");
        let response = put_config(
            State(state),
            Query(ConfigQuery::default()),
            headers,
            Body::from(mine),
        )
        .await;

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let payload: serde_json::Value =
            serde_json::from_str(&response_text(response).await).expect("json conflict");
        assert_eq!(payload["error"], "config_changed");
        assert_eq!(payload["expected_etag"], stale_etag);
        assert_eq!(
            payload["current_toml"], theirs,
            "the conflict carries the other version so the UI can diff all three"
        );
        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            theirs,
            "a rejected apply must not touch the file"
        );
    }

    #[tokio::test]
    async fn an_apply_with_a_current_if_match_goes_through() {
        let (_dir, config_path) = seeded(&sample_config("127.0.0.1:8080"));
        let state = admin_state(&config_path);

        let loaded = get_config(State(state.clone()), Query(ConfigQuery::default())).await;
        let etag = loaded
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let mut headers = header::HeaderMap::new();
        headers.insert(header::IF_MATCH, HeaderValue::from_str(&etag).unwrap());
        let next = sample_config("127.0.0.1:8081");
        let response = put_config(
            State(state),
            Query(ConfigQuery::default()),
            headers,
            Body::from(next.clone()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let new_etag = response
            .headers()
            .get(header::ETAG)
            .expect("the new version answers with its own ETag")
            .to_str()
            .unwrap()
            .to_string();
        assert_ne!(new_etag, etag);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), next);
    }

    #[tokio::test]
    async fn a_dry_run_answers_with_the_report_and_writes_nothing() {
        let original = sample_config("127.0.0.1:8080");
        let (_dir, config_path) = seeded(&original);
        let state = admin_state(&config_path);

        let response = put_config(
            State(state),
            Query(ConfigQuery {
                format: None,
                dry_run: Some("true".to_string()),
            }),
            header::HeaderMap::new(),
            Body::from(sample_config("127.0.0.1:8081")),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value =
            serde_json::from_str(&response_text(response).await).expect("json report");
        assert_eq!(payload["valid"], true);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[tokio::test]
    async fn an_invalid_apply_names_the_line_it_failed_on() {
        let original = sample_config("127.0.0.1:8080");
        let (_dir, config_path) = seeded(&original);
        let state = admin_state(&config_path);

        let response = put_config(
            State(state),
            Query(ConfigQuery::default()),
            header::HeaderMap::new(),
            Body::from("[server]\nlisten = [\n"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_text(response).await;
        assert!(body.starts_with("invalid_config: line "), "{body}");
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }
}
