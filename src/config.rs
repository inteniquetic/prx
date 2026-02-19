use std::{fs, path::Path};

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PrxConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(rename = "route", default)]
    pub routes: Vec<RouteConfig>,
}

impl PrxConfig {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.to_string_lossy()))?;
        let config: Self = toml::from_str(&content).with_context(|| {
            format!(
                "failed to parse TOML config from {}",
                path.to_string_lossy()
            )
        })?;
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

        let mut defaults = 0usize;
        for route in &self.routes {
            if route.is_default {
                defaults += 1;
            }
            if route.upstreams.is_empty() {
                bail!(
                    "route '{}' must include at least one [[route.upstream]]",
                    route.name
                );
            }

            if route.path_prefix.is_empty() {
                bail!("route '{}' has empty path_prefix", route.name);
            }
            if !route.path_prefix.starts_with('/') {
                bail!("route '{}' path_prefix must start with '/'", route.name);
            }

            for upstream in &route.upstreams {
                if upstream.addr.trim().is_empty() {
                    bail!("route '{}' includes upstream with empty addr", route.name);
                }
            }

            if route.circuit_breaker.enabled {
                if route.circuit_breaker.consecutive_failures == 0 {
                    bail!(
                        "route '{}' circuit_breaker.consecutive_failures must be > 0",
                        route.name
                    );
                }
                if route.circuit_breaker.open_ms == 0 {
                    bail!("route '{}' circuit_breaker.open_ms must be > 0", route.name);
                }
            }
        }

        if defaults > 1 {
            bail!("only one route can be marked is_default = true");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub listen: String,
    pub cert_path: String,
    pub key_path: String,
    #[serde(default = "default_true")]
    pub enable_h2: bool,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct RouteConfig {
    #[serde(default = "default_route_name")]
    pub name: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_path_prefix")]
    pub path_prefix: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub lb: LbStrategy,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default)]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    #[serde(rename = "upstream", default)]
    pub upstreams: Vec<UpstreamConfig>,
}

fn default_route_name() -> String {
    "default".to_string()
}

fn default_path_prefix() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LbStrategy {
    #[default]
    RoundRobin,
    Random,
    Hash,
}

#[derive(Debug, Clone, Deserialize)]
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

fn default_cb_failures() -> usize {
    3
}

fn default_cb_open_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, Deserialize)]
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

    fn valid_route() -> RouteConfig {
        RouteConfig {
            name: "default".to_string(),
            host: None,
            path_prefix: "/".to_string(),
            is_default: true,
            lb: LbStrategy::RoundRobin,
            max_retries: 0,
            retry_backoff_ms: 0,
            circuit_breaker: CircuitBreakerConfig::default(),
            upstreams: vec![UpstreamConfig {
                addr: "127.0.0.1:8081".to_string(),
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
            }],
        }
    }

    #[test]
    fn validate_rejects_invalid_health_path() {
        let mut cfg = PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            routes: vec![valid_route()],
        };
        cfg.server.health_path = "healthz".to_string();

        let err = cfg.validate().expect_err("invalid health_path should fail");
        assert!(err.to_string().contains("server.health_path"));
    }

    #[test]
    fn validate_rejects_invalid_circuit_breaker_config() {
        let mut cfg = PrxConfig {
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            routes: vec![valid_route()],
        };
        cfg.routes[0].circuit_breaker.enabled = true;
        cfg.routes[0].circuit_breaker.consecutive_failures = 0;

        let err = cfg
            .validate()
            .expect_err("invalid circuit breaker threshold should fail");
        assert!(err.to_string().contains("consecutive_failures"));
    }
}
