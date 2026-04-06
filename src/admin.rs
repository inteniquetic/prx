use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::{
    Router,
    body::{self, Body},
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use include_dir::{Dir, include_dir};
use pingora::services::Service;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    config::{LbStrategy, PrxConfig},
    runtime::RuntimeConfig,
};

pub const ADMIN_CONFIG_PATH: &str = "/web/config";
pub const ADMIN_ROUTE_HEALTH_PATH: &str = "/web/health/routes";
pub const DEFAULT_ADMIN_LISTEN: &str = "127.0.0.1:9090";
const MAX_ADMIN_CONFIG_BODY_BYTES: usize = 10 * 1024 * 1024;
pub const ADMIN_SERVICES_PATH: &str = "/admin/services";
pub const ADMIN_SERVICES_NAME_PATH: &str = "/admin/services/:name";
pub const ADMIN_ROUTES_PATH: &str = "/admin/routes";
pub const ADMIN_ROUTES_NAME_PATH: &str = "/admin/routes/:name";
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

    pub fn modify_config<F>(&self, active_config: &Arc<ArcSwap<RuntimeConfig>>, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut PrxConfig) -> anyhow::Result<()>,
    {
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
}

#[derive(Debug, Default, Deserialize)]
struct ConfigQuery {
    format: Option<String>,
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
    cert_path: String,
    key_path: String,
    enable_h2: bool,
}

#[derive(Debug, Serialize)]
struct AdminObservabilityPayload {
    log_level: String,
    access_log: bool,
    prometheus_listen: String,
}

#[derive(Debug, Serialize)]
struct AdminServicePayload {
    name: String,
    lb: String,
    max_retries: usize,
    retry_backoff_ms: u64,
    circuit_breaker: AdminCircuitBreakerPayload,
    upstreams: Vec<AdminUpstreamPayload>,
}

#[derive(Debug, Serialize)]
struct AdminRoutePayload {
    name: String,
    service: String,
    host: String,
    path_prefix: String,
    methods: Vec<String>,
    is_default: bool,
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
    #[serde(default)]
    pub circuit_breaker: Option<CircuitBreakerRequestPayload>,
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
            tls: config.server.tls.map(|tls| AdminTlsPayload {
                listen: tls.listen,
                cert_path: tls.cert_path,
                key_path: tls.key_path,
                enable_h2: tls.enable_h2,
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
            .map(|service| AdminServicePayload {
                name: service.name.clone(),
                lb: lb_to_string(service.lb.clone()).to_string(),
                max_retries: service.max_retries,
                retry_backoff_ms: service.retry_backoff_ms,
                circuit_breaker: AdminCircuitBreakerPayload {
                    enabled: service.circuit_breaker.enabled,
                    consecutive_failures: service.circuit_breaker.consecutive_failures,
                    open_ms: service.circuit_breaker.open_ms,
                },
                upstreams: service
                    .upstreams
                    .iter()
                    .map(|upstream| AdminUpstreamPayload {
                        addr: upstream.addr.clone(),
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
                    })
                    .collect(),
            })
            .collect();

        let routes = config
            .routes
            .iter()
            .map(|route| AdminRoutePayload {
                name: route.name.clone(),
                service: route.service.clone(),
                host: route.host.clone().unwrap_or_default(),
                path_prefix: route.path_prefix.clone(),
                methods: route.methods.clone(),
                is_default: route.is_default,
            })
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
        },
        Ok(Err(err)) => RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: false,
            latency_ms: None,
            error: Some(err.to_string()),
        },
        Err(_) => RouteHealthUpstreamPayload {
            addr,
            timeout_ms,
            healthy: false,
            latency_ms: None,
            error: Some("timeout".to_string()),
        },
    }
}

async fn render_route_health_payload(config: PrxConfig, timeout_ms: u64) -> RouteHealthPayload {
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
                let per_upstream_timeout_ms = health_timeout_ms(upstream.connect_timeout_ms);
                upstream_payloads
                    .push(check_upstream_health(upstream.addr.clone(), per_upstream_timeout_ms).await);
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

    let payload = render_route_health_payload(config, timeout_ms).await;
    json_response(StatusCode::OK, &payload)
}

async fn post_route_health(
    State(_state): State<AdminState>,
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

    let payload = render_route_health_payload(config, timeout_ms).await;
    json_response(StatusCode::OK, &payload)
}

fn lb_to_string(lb: LbStrategy) -> &'static str {
    match lb {
        LbStrategy::RoundRobin => "round_robin",
        LbStrategy::Random => "random",
        LbStrategy::Hash => "hash",
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

    // SPA fallback for client-side routes.
    if !normalized.contains('.') {
        return fallback_index();
    }

    text_response(StatusCode::NOT_FOUND, b"not_found\n".to_vec())
}

async fn get_config(
    State(state): State<AdminState>,
    Query(query): Query<ConfigQuery>,
) -> Response<Body> {
    if query
        .format
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        return match state.config_admin.read_parsed_config() {
            Ok(config) => json_response(StatusCode::OK, &AdminConfigPayload::from(config)),
            Err(err) => text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed_to_read_config: {err:#}\n"),
            ),
        };
    }

    match state.config_admin.read_config_text() {
        Ok(content) => text_response(StatusCode::OK, content.into_bytes()),
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_read_config: {err:#}\n"),
        ),
    }
}

async fn put_config(State(state): State<AdminState>, body: Body) -> Response<Body> {
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

    if let Err(err) = PrxConfig::from_toml_str(text) {
        return text_response(
            StatusCode::BAD_REQUEST,
            format!("invalid_config: {err:#}\n"),
        );
    }

    match state
        .config_admin
        .apply_config_text(text, &state.active_config)
    {
        Ok(()) => text_response(StatusCode::OK, b"config_applied\n".to_vec()),
        Err(err) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed_to_apply_config: {err:#}\n"),
        ),
    }
}

async fn get_webui_root() -> Response<Body> {
    handle_webui_get("")
}

async fn get_webui_path(AxumPath(path): AxumPath<String>) -> Response<Body> {
    handle_webui_get(path.as_str())
}

// ==================== Service CRUD Handlers ====================

async fn list_services(
    State(state): State<AdminState>,
) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            let services: Vec<AdminServicePayload> = config.services.iter().map(|s| {
                AdminServicePayload {
                    name: s.name.clone(),
                    lb: lb_to_string(s.lb.clone()).to_string(),
                    max_retries: s.max_retries,
                    retry_backoff_ms: s.retry_backoff_ms,
                    circuit_breaker: AdminCircuitBreakerPayload {
                        enabled: s.circuit_breaker.enabled,
                        consecutive_failures: s.circuit_breaker.consecutive_failures,
                        open_ms: s.circuit_breaker.open_ms,
                    },
                    upstreams: s.upstreams.iter().map(|u| {
                        AdminUpstreamPayload {
                            addr: u.addr.clone(),
                            tls: u.tls,
                            sni: u.sni.clone().unwrap_or_default(),
                            weight: u.weight,
                            verify_cert: u.verify_cert,
                            verify_hostname: u.verify_hostname,
                            connect_timeout_ms: u.connect_timeout_ms,
                            total_connect_timeout_ms: u.total_connect_timeout_ms,
                            read_timeout_ms: u.read_timeout_ms,
                            write_timeout_ms: u.write_timeout_ms,
                            idle_timeout_ms: u.idle_timeout_ms,
                        }
                    }).collect(),
                }
            }).collect();
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
                let service_payload = AdminServicePayload {
                    name: service.name.clone(),
                    lb: lb_to_string(service.lb.clone()).to_string(),
                    max_retries: service.max_retries,
                    retry_backoff_ms: service.retry_backoff_ms,
                    circuit_breaker: AdminCircuitBreakerPayload {
                        enabled: service.circuit_breaker.enabled,
                        consecutive_failures: service.circuit_breaker.consecutive_failures,
                        open_ms: service.circuit_breaker.open_ms,
                    },
                    upstreams: service.upstreams.iter().map(|u| {
                        AdminUpstreamPayload {
                            addr: u.addr.clone(),
                            tls: u.tls,
                            sni: u.sni.clone().unwrap_or_default(),
                            weight: u.weight,
                            verify_cert: u.verify_cert,
                            verify_hostname: u.verify_hostname,
                            connect_timeout_ms: u.connect_timeout_ms,
                            total_connect_timeout_ms: u.total_connect_timeout_ms,
                            read_timeout_ms: u.read_timeout_ms,
                            write_timeout_ms: u.write_timeout_ms,
                            idle_timeout_ms: u.idle_timeout_ms,
                        }
                    }).collect(),
                };
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

async fn create_service(
    State(state): State<AdminState>,
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

    if payload.name.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"service_name_cannot_be_empty\n".to_vec());
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
    if let Some(cb) = &payload.circuit_breaker {
        if cb.enabled.unwrap_or(false) {
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
    }

    match state.config_admin.modify_config(&state.active_config, |config| {
        // Check for duplicate service name
        if config.services.iter().any(|s| s.name == payload.name) {
            return Err(anyhow::anyhow!("service '{}' already exists", payload.name));
        }

        let service = crate::config::ServiceConfig {
            name: payload.name.clone(),
            lb: payload.lb.as_deref().map(|s| s.parse().unwrap_or_default())
                .unwrap_or_default(),
            max_retries: payload.max_retries.unwrap_or(0),
            retry_backoff_ms: payload.retry_backoff_ms.unwrap_or(0),
            circuit_breaker: payload.circuit_breaker.map(|cb| crate::config::CircuitBreakerConfig {
                enabled: cb.enabled.unwrap_or(false),
                consecutive_failures: cb.consecutive_failures.unwrap_or_default(),
                open_ms: cb.open_ms.unwrap_or_default(),
            }).unwrap_or_default(),
            upstreams: payload.upstreams.into_iter().map(|u| crate::config::UpstreamConfig {
                addr: u.addr,
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
            }).collect(),
        };

        config.services.push(service);
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::CREATED, b"service_created\n".to_vec()),
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
    if let Some(cb) = &payload.circuit_breaker {
        if cb.enabled.unwrap_or(false) {
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
    }

    match state.config_admin.modify_config(&state.active_config, |config| {
        let index = config.services.iter().position(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("service '{}' not found", name))?;

        let service = crate::config::ServiceConfig {
            name: payload.name.clone(),
            lb: payload.lb.as_deref().map(|s| s.parse().unwrap_or_default())
                .unwrap_or_else(|| config.services[index].lb.clone()),
            max_retries: payload.max_retries.unwrap_or(config.services[index].max_retries),
            retry_backoff_ms: payload.retry_backoff_ms.unwrap_or(config.services[index].retry_backoff_ms),
            circuit_breaker: payload.circuit_breaker.map(|cb| crate::config::CircuitBreakerConfig {
                enabled: cb.enabled.unwrap_or(config.services[index].circuit_breaker.enabled),
                consecutive_failures: cb.consecutive_failures.unwrap_or(config.services[index].circuit_breaker.consecutive_failures),
                open_ms: cb.open_ms.unwrap_or(config.services[index].circuit_breaker.open_ms),
            }).unwrap_or_else(|| config.services[index].circuit_breaker.clone()),
            upstreams: payload.upstreams.into_iter().map(|u| crate::config::UpstreamConfig {
                addr: u.addr,
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
            }).collect(),
        };

        config.services[index] = service;
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::OK, b"service_updated\n".to_vec()),
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
    match state.config_admin.modify_config(&state.active_config, |config| {
        let index = config.services.iter().position(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("service '{}' not found", name))?;

        // Check if any routes reference this service
        let referenced = config.routes.iter().any(|r| r.service == name);
        if referenced {
            return Err(anyhow::anyhow!(
                "service '{}' is referenced by one or more routes",
                name
            ));
        }

        config.services.remove(index);
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::OK, b"service_deleted\n".to_vec()),
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

async fn list_routes(
    State(state): State<AdminState>,
) -> Response<Body> {
    match state.config_admin.read_parsed_config() {
        Ok(config) => {
            let routes: Vec<AdminRoutePayload> = config.routes.iter().map(|r| {
                AdminRoutePayload {
                    name: r.name.clone(),
                    service: r.service.clone(),
                    host: r.host.clone().unwrap_or_default(),
                    path_prefix: r.path_prefix.clone(),
                    methods: r.methods.clone(),
                    is_default: r.is_default,
                }
            }).collect();
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
                let route_payload = AdminRoutePayload {
                    name: route.name.clone(),
                    service: route.service.clone(),
                    host: route.host.clone().unwrap_or_default(),
                    path_prefix: route.path_prefix.clone(),
                    methods: route.methods.clone(),
                    is_default: route.is_default,
                };
                json_response(StatusCode::OK, &route_payload)
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

async fn create_route(
    State(state): State<AdminState>,
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

    if payload.name.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"route_name_cannot_be_empty\n".to_vec());
    }

    if payload.service.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, b"service_cannot_be_empty\n".to_vec());
    }

    let path_prefix = payload.path_prefix.as_deref().unwrap_or("/");
    if path_prefix.is_empty() || !path_prefix.starts_with('/') {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_path_prefix_must_be_non_empty_and_start_with_slash\n".to_vec(),
        );
    }

    match state.config_admin.modify_config(&state.active_config, |config| {
        // Check for duplicate route name
        if config.routes.iter().any(|r| r.name == payload.name) {
            return Err(anyhow::anyhow!("route '{}' already exists", payload.name));
        }

        // Check if service exists
        if !config.services.iter().any(|s| s.name == payload.service) {
            return Err(anyhow::anyhow!("service '{}' not found", payload.service));
        }

        // Check for duplicate default route
        if payload.is_default.unwrap_or(false) {
            if config.routes.iter().any(|r| r.is_default) {
                return Err(anyhow::anyhow!("only one route can be marked as default"));
            }
        }

        let route = crate::config::RouteConfig {
            name: payload.name.clone(),
            service: payload.service.clone(),
            host: payload.host,
            path_prefix: payload.path_prefix.unwrap_or_else(|| "/".to_string()),
            methods: payload.methods.unwrap_or_default(),
            is_default: payload.is_default.unwrap_or(false),
        };

        config.routes.push(route);
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::CREATED, b"route_created\n".to_vec()),
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
        return text_response(StatusCode::BAD_REQUEST, b"service_cannot_be_empty\n".to_vec());
    }

    let path_prefix = payload.path_prefix.as_deref().unwrap_or("/");
    if path_prefix.is_empty() || !path_prefix.starts_with('/') {
        return text_response(
            StatusCode::BAD_REQUEST,
            b"route_path_prefix_must_be_non_empty_and_start_with_slash\n".to_vec(),
        );
    }

    match state.config_admin.modify_config(&state.active_config, |config| {
        let index = config.routes.iter().position(|r| r.name == name)
            .ok_or_else(|| anyhow::anyhow!("route '{}' not found", name))?;

        // Check if service exists
        if !config.services.iter().any(|s| s.name == payload.service) {
            return Err(anyhow::anyhow!("service '{}' not found", payload.service));
        }

        // Check for duplicate default route
        let current_default = config.routes[index].is_default;
        let new_default = payload.is_default.unwrap_or(current_default);
        if new_default && !current_default {
            if config.routes.iter().any(|r| r.is_default && r.name != name) {
                return Err(anyhow::anyhow!("only one route can be marked as default"));
            }
        }

        let route = crate::config::RouteConfig {
            name: payload.name.clone(),
            service: payload.service.clone(),
            host: payload.host,
            path_prefix: payload.path_prefix.unwrap_or_else(|| "/".to_string()),
            methods: payload.methods.unwrap_or_else(|| config.routes[index].methods.clone()),
            is_default: payload.is_default.unwrap_or(config.routes[index].is_default),
        };

        config.routes[index] = route;
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::OK, b"route_updated\n".to_vec()),
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
    match state.config_admin.modify_config(&state.active_config, |config| {
        let index = config.routes.iter().position(|r| r.name == name)
            .ok_or_else(|| anyhow::anyhow!("route '{}' not found", name))?;

        config.routes.remove(index);
        Ok(())
    }) {
        Ok(_) => text_response(StatusCode::OK, b"route_deleted\n".to_vec()),
        Err(err) => {
            if err.to_string().contains("not found") {
                text_response(StatusCode::NOT_FOUND, format!("{err:#}\n"))
            } else {
                text_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}\n"))
            }
        }
    }
}

fn build_router(state: AdminState) -> Router {
    Router::new()
        // Config endpoints
        .route(ADMIN_CONFIG_PATH, get(get_config).put(put_config))
        .route(
            ADMIN_ROUTE_HEALTH_PATH,
            get(get_route_health).post(post_route_health),
        )
        // Service CRUD endpoints
        .route(ADMIN_SERVICES_PATH, get(list_services).post(create_service))
        .route(ADMIN_SERVICES_NAME_PATH, get(get_service).put(update_service).delete(delete_service))
        // Route CRUD endpoints
        .route(ADMIN_ROUTES_PATH, get(list_routes).post(create_route))
        .route(ADMIN_ROUTES_NAME_PATH, get(get_route).put(update_route).delete(delete_route))
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
    ) -> Self {
        Self {
            name: "prx-admin-axum".to_string(),
            listen,
            listener: Some(listener),
            state: AdminState {
                config_admin: ConfigAdmin::new(config_path),
                active_config,
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
}