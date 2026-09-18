use std::{fs, path::Path};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrxConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub headers: HeadersConfig,
    #[serde(rename = "service", default)]
    pub services: Vec<ServiceConfig>,
    #[serde(rename = "route", default)]
    pub routes: Vec<RouteConfig>,
}

impl PrxConfig {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.to_string_lossy()))?;
        Self::from_toml_str(&content).with_context(|| {
            format!(
                "failed to parse TOML config from {}",
                path.to_string_lossy()
            )
        })
    }

    pub fn from_toml_str(content: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(content).context("invalid TOML config")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.routes.is_empty() {
            bail!("config must include at least one [[route]] block");
        }

        if !self.server.health_path.starts_with('/') {
            bail!("server.health_path must start with '/'");
        }
        if !self.server.ready_path.starts_with('/') {
            bail!("server.ready_path must start with '/'");
        }
        if self.server.health_path == self.server.ready_path {
            bail!("server.health_path and server.ready_path must be different");
        }

        // Validate services
        let mut service_names = std::collections::HashSet::new();
        for service in &self.services {
            if service_names.contains(&service.name) {
                bail!("duplicate service name '{}'", service.name);
            }
            service_names.insert(service.name.clone());

            if service.upstreams.is_empty() {
                bail!(
                    "service '{}' must include at least one [[service.upstream]]",
                    service.name
                );
            }

            for upstream in &service.upstreams {
                if upstream.addr.trim().is_empty() {
                    bail!(
                        "service '{}' includes upstream with empty addr",
                        service.name
                    );
                }
            }

            if service.circuit_breaker.enabled {
                if service.circuit_breaker.consecutive_failures == 0 {
                    bail!(
                        "service '{}' circuit_breaker.consecutive_failures must be > 0",
                        service.name
                    );
                }
                if service.circuit_breaker.open_ms == 0 {
                    bail!(
                        "service '{}' circuit_breaker.open_ms must be > 0",
                        service.name
                    );
                }
            }
        }

        // Validate routes
        let mut defaults = 0usize;
        for route in &self.routes {
            if route.is_default {
                defaults += 1;
            }

            if route.path_prefix.is_empty() {
                bail!("route '{}' has empty path_prefix", route.name);
            }
            if !route.path_prefix.starts_with('/') {
                bail!("route '{}' path_prefix must start with '/'", route.name);
            }

            if !service_names.contains(&route.service) {
                bail!(
                    "route '{}' references unknown service '{}'",
                    route.name,
                    route.service
                );
            }

            validate_header_rules(&route.request_headers, &format!("route '{}'", route.name))?;
            validate_header_rules(&route.response_headers, &format!("route '{}'", route.name))?;

            // An unknown method would silently make the route unreachable, so
            // reject it while the config is being loaded instead.
            for method in &route.methods {
                if crate::router::method_bit(method).is_none() {
                    bail!(
                        "route '{}' lists unsupported HTTP method '{}' (supported: \
                         GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE, CONNECT)",
                        route.name,
                        method
                    );
                }
            }
        }

        if defaults > 1 {
            bail!("only one route can be marked is_default = true");
        }

        validate_header_rules(&self.headers.request, "headers.request")?;
        validate_header_rules(&self.headers.response, "headers.response")?;

        Ok(())
    }
}

/// Variables that can appear inside a header value.
pub const HEADER_VARIABLES: [&str; 7] = [
    "client_ip",
    "client_port",
    "scheme",
    "host",
    "route_name",
    "upstream_addr",
    "request_id",
];

fn validate_header_rules(rules: &HeaderRules, context: &str) -> anyhow::Result<()> {
    let check_name = |name: &str| -> anyhow::Result<()> {
        if http::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            bail!("{context} has an invalid header name '{name}'");
        }
        Ok(())
    };

    for (name, value) in rules.set.iter().chain(rules.add.iter()) {
        check_name(name)?;
        for variable in extract_variables(value) {
            if !HEADER_VARIABLES.contains(&variable.as_str()) {
                bail!(
                    "{context} header '{name}' uses unknown variable '${variable}' \
                     (supported: {})",
                    HEADER_VARIABLES
                        .iter()
                        .map(|v| format!("${v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }
    for name in &rules.remove {
        check_name(name)?;
    }
    Ok(())
}

/// Returns the variable names used in a header value template.
pub fn extract_variables(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = value.as_bytes();
    let mut idx = 0;
    while let Some(pos) = value[idx..].find('$') {
        let start = idx + pos + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end > start {
            found.push(value[start..end].to_string());
        }
        idx = end.max(start);
        if idx >= value.len() {
            break;
        }
    }
    found
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: Vec<String>,
    #[serde(default = "default_health_path")]
    pub health_path: String,
    #[serde(default = "default_ready_path")]
    pub ready_path: String,
    #[serde(default)]
    pub threads: Option<usize>,
    #[serde(default)]
    pub grace_period_seconds: Option<u64>,
    #[serde(default)]
    pub graceful_shutdown_timeout_seconds: Option<u64>,
    #[serde(default = "default_reload_debounce_ms")]
    pub config_reload_debounce_ms: u64,
    /// Accept HTTP/2 over cleartext on the plain listeners. prx peeks for the
    /// h2 preface and falls back to HTTP/1.1, so leaving this on is safe and
    /// is what gRPC clients that do not use TLS need.
    #[serde(default = "default_true")]
    pub h2c: bool,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            health_path: default_health_path(),
            ready_path: default_ready_path(),
            threads: None,
            grace_period_seconds: None,
            graceful_shutdown_timeout_seconds: None,
            config_reload_debounce_ms: default_reload_debounce_ms(),
            h2c: true,
            tls: None,
        }
    }
}

fn default_listen() -> Vec<String> {
    vec!["0.0.0.0:8080".to_string()]
}

fn default_reload_debounce_ms() -> u64 {
    250
}

fn default_health_path() -> String {
    "/healthz".to_string()
}

fn default_ready_path() -> String {
    "/readyz".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub listen: String,
    pub cert_path: String,
    pub key_path: String,
    #[serde(default = "default_true")]
    pub enable_h2: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_true")]
    pub access_log: bool,
    #[serde(default)]
    pub prometheus_listen: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            access_log: true,
            prometheus_listen: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    #[serde(default = "default_service_name")]
    pub name: String,
    #[serde(default)]
    pub lb: LbStrategy,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default)]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    /// Which HTTP version to speak to the upstreams of this service.
    #[serde(default)]
    pub upstream_h2: UpstreamH2,
    #[serde(rename = "upstream", default)]
    pub upstreams: Vec<UpstreamConfig>,
}

/// How prx talks to a service's upstreams.
///
/// gRPC needs `always`: it requires end-to-end HTTP/2 because it carries its
/// status in trailers, which HTTP/1.1 cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamH2 {
    /// HTTP/1.1 only.
    #[default]
    Never,
    /// HTTP/2 only. Over TLS this is negotiated with ALPN; over cleartext prx
    /// trusts that the upstream speaks h2c.
    Always,
    /// Prefer HTTP/2, fall back to HTTP/1.1. Over cleartext there is no ALPN to
    /// negotiate with, so this behaves like `never`.
    Auto,
}

fn default_service_name() -> String {
    "default".to_string()
}

/// Header rules applied to a request or a response.
///
/// `set` replaces any existing value, `add` appends another value, and
/// `remove` drops the header. They are applied in that order, so a rule set
/// can normalize a header and then append to it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HeaderRules {
    #[serde(default)]
    pub set: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub add: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

impl HeaderRules {
    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.add.is_empty() && self.remove.is_empty()
    }
}

/// Header rules that apply to every route, applied before the route's own.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HeadersConfig {
    #[serde(default)]
    pub request: HeaderRules,
    #[serde(default)]
    pub response: HeaderRules,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteConfig {
    #[serde(default = "default_route_name")]
    pub name: String,
    pub service: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_path_prefix")]
    pub path_prefix: String,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub request_headers: HeaderRules,
    #[serde(default)]
    pub response_headers: HeaderRules,
}

fn default_route_name() -> String {
    "default".to_string()
}

fn default_path_prefix() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LbStrategy {
    #[default]
    RoundRobin,
    Random,
    Hash,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cb_failures")]
    pub consecutive_failures: usize,
    #[serde(default = "default_cb_open_ms")]
    pub open_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            consecutive_failures: default_cb_failures(),
            open_ms: default_cb_open_ms(),
        }
    }
}

impl std::str::FromStr for LbStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "round_robin" => Ok(LbStrategy::RoundRobin),
            "random" => Ok(LbStrategy::Random),
            "hash" => Ok(LbStrategy::Hash),
            _ => Err(format!("invalid load balancing strategy: {}", s)),
        }
    }
}

fn default_cb_failures() -> usize {
    3
}

fn default_cb_open_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpstreamConfig {
    pub addr: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub sni: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u16,
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

fn default_weight() -> u16 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_upstream(addr: &str) -> UpstreamConfig {
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

    fn valid_service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            lb: LbStrategy::RoundRobin,
            upstream_h2: UpstreamH2::default(),
            max_retries: 0,
            retry_backoff_ms: 0,
            circuit_breaker: CircuitBreakerConfig::default(),
            upstreams: vec![valid_upstream("127.0.0.1:8081")],
        }
    }

    fn valid_route(name: &str, service: &str) -> RouteConfig {
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

    fn valid_config() -> PrxConfig {
        PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
            services: vec![valid_service("default")],
            routes: vec![valid_route("default", "default")],
        }
    }

    #[test]
    fn validate_rejects_unknown_http_method() {
        let mut cfg = valid_config();
        cfg.routes[0].methods = vec!["GET".to_string(), "FETCH".to_string()];

        let err = cfg.validate().expect_err("unknown method should fail");
        assert!(err.to_string().contains("FETCH"), "{err}");
    }

    #[test]
    fn validate_accepts_known_http_methods_in_any_case() {
        let mut cfg = valid_config();
        cfg.routes[0].methods = vec!["get".to_string(), "Post".to_string()];

        cfg.validate()
            .expect("known methods should pass validation");
    }

    #[test]
    fn validate_rejects_invalid_health_path() {
        let mut cfg = valid_config();
        cfg.server.health_path = "healthz".to_string();

        let err = cfg.validate().expect_err("invalid health_path should fail");
        assert!(err.to_string().contains("server.health_path"));
    }

    #[test]
    fn validate_rejects_invalid_circuit_breaker_config() {
        let mut cfg = valid_config();
        cfg.services[0].circuit_breaker.enabled = true;
        cfg.services[0].circuit_breaker.consecutive_failures = 0;

        let err = cfg
            .validate()
            .expect_err("invalid circuit breaker threshold should fail");
        assert!(err.to_string().contains("consecutive_failures"));
    }

    #[test]
    fn validate_rejects_route_with_unknown_service() {
        let mut cfg = valid_config();
        cfg.routes[0].service = "nonexistent".to_string();

        let err = cfg
            .validate()
            .expect_err("route with unknown service should fail");
        assert!(err.to_string().contains("unknown service"));
    }

    #[test]
    fn validate_rejects_service_with_no_upstreams() {
        let mut cfg = valid_config();
        cfg.services[0].upstreams.clear();

        let err = cfg
            .validate()
            .expect_err("service with no upstreams should fail");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_rejects_duplicate_service_names() {
        let mut cfg = valid_config();
        cfg.services.push(valid_service("default"));

        let err = cfg
            .validate()
            .expect_err("duplicate service names should fail");
        assert!(err.to_string().contains("duplicate service name"));
    }

    #[test]
    fn validate_accepts_valid_config() {
        let cfg = valid_config();
        cfg.validate().expect("valid config should pass");
    }
}
