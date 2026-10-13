use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct PrxConfig {
    #[serde(default)]
    pub server: ServerConfig,
    /// The control plane: where the admin API listens and how it is guarded.
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub headers: HeadersConfig,
    #[serde(default)]
    pub compression: CompressionConfig,
    #[serde(rename = "service", default)]
    pub services: Vec<ServiceConfig>,
    #[serde(rename = "route", default)]
    pub routes: Vec<RouteConfig>,
    /// Plugins are declared once here and referenced by name from the routes
    /// that want them, the same way a route references a service (T501).
    #[serde(rename = "plugin", default)]
    pub plugins: Vec<PluginConfig>,
    /// SecLang rules for the WAF. Empty unless `[waf] rules_file` is set.
    #[serde(default, skip_serializing_if = "WafConfig::is_unset")]
    pub waf: WafConfig,
}

/// The `[waf]` block. Absent means no rules, so the WAF does nothing.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct WafConfig {
    /// A SecLang rules file. A relative path resolves against the directory
    /// of the config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_file: Option<PathBuf>,
    /// CRS paranoia level, 1 to 4. Only rules at or below this level count
    /// toward blocking. Unset means 1. Kept as `i64` so that out-of-range
    /// values reach `check` as a `ConfigIssue` instead of a parse error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paranoia_level: Option<i64>,
    /// Summed anomaly score at which a request is blocked. Must be positive.
    /// Unset means 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_anomaly_threshold: Option<i64>,
    /// Directory of the config file, set by `PrxConfig::from_file`. Not TOML.
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
}

/// Defaults for the optional `[waf]` tuning keys.
pub const DEFAULT_PARANOIA_LEVEL: u8 = 1;
pub const DEFAULT_INBOUND_ANOMALY_THRESHOLD: u32 = 5;

impl WafConfig {
    fn is_unset(&self) -> bool {
        self.rules_file.is_none()
            && self.paranoia_level.is_none()
            && self.inbound_anomaly_threshold.is_none()
    }

    /// The paranoia level in effect. Values outside 1 to 4 are rejected by
    /// `check`, so only the default can be read here for an invalid config.
    pub fn paranoia_level(&self) -> u8 {
        self.paranoia_level
            .and_then(|level| u8::try_from(level).ok())
            .filter(|level| (1..=4).contains(level))
            .unwrap_or(DEFAULT_PARANOIA_LEVEL)
    }

    /// The inbound anomaly threshold in effect. Non-positive values are
    /// rejected by `check`.
    pub fn inbound_anomaly_threshold(&self) -> u32 {
        self.inbound_anomaly_threshold
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_INBOUND_ANOMALY_THRESHOLD)
    }

    /// The rules file to load, with a relative path made relative to the
    /// config file's directory.
    pub fn resolved_rules_file(&self) -> Option<PathBuf> {
        let file = self.rules_file.as_ref()?;
        if file.is_absolute() {
            return Some(file.clone());
        }
        Some(match &self.base_dir {
            Some(dir) => dir.join(file),
            None => file.clone(),
        })
    }
}

/// One `[[plugin]]` block.
///
/// `config` is left as a raw TOML value because what belongs in it is decided
/// by `kind`: the registry checks it, not this struct. Keeping it opaque here
/// is what stops every new plugin from having to add a field to the core
/// config type.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct PluginConfig {
    pub name: String,
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "empty_table")]
    #[schemars(with = "serde_json::Value")]
    pub config: toml::Value,
}

fn empty_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

impl PrxConfig {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.to_string_lossy()))?;
        Self::from_text_at(&content, path)
    }

    /// [`Self::from_file`] for text that is not on disk yet: what a write
    /// would produce, judged as if it lived at `path`, so a relative
    /// `rules_file` resolves the same way.
    pub fn from_text_at(content: &str, path: &Path) -> anyhow::Result<Self> {
        // The base directory has to be known before validation: `check_waf`
        // resolves a relative rules_file against it.
        let mut config = Self::parse_toml(content).with_context(|| {
            format!(
                "failed to parse TOML config from {}",
                path.to_string_lossy()
            )
        })?;
        config.waf.base_dir = path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf);
        config.validate().with_context(|| {
            format!(
                "failed to parse TOML config from {}",
                path.to_string_lossy()
            )
        })?;
        Ok(config)
    }

    pub fn from_toml_str(content: &str) -> anyhow::Result<Self> {
        let config = Self::parse_toml(content)?;
        config.validate()?;
        Ok(config)
    }

    fn parse_toml(content: &str) -> anyhow::Result<Self> {
        toml::from_str(content).context("invalid TOML config")
    }

    /// The first error in the config, as an `Err`.
    ///
    /// Everything is checked in one pass (`check`): the editor marks every
    /// mistake at once, while a caller that is only loading a file still gets a
    /// single error back.
    pub fn validate(&self) -> anyhow::Result<()> {
        let issues = self.check();
        if let Some(first) = issues
            .iter()
            .find(|issue| issue.severity == IssueSeverity::Error)
        {
            bail!("{}", first.message);
        }
        Ok(())
    }

    /// Every problem in the config — errors and warnings — in file order.
    ///
    /// Errors are what stops a config from being applied. Warnings are things
    /// that load fine but rarely mean what they say: a route nothing can reach,
    /// a service nothing routes to, a TLS upstream with verification off.
    pub fn check(&self) -> Vec<ConfigIssue> {
        let mut out = Vec::new();
        self.check_server(&mut out);
        let service_names = self.check_services(&mut out);
        self.check_routes(&service_names, &mut out);
        self.check_plugins(&mut out);
        self.check_waf(&mut out);
        self.check_waf_tuning(&mut out);
        self.check_admin(&mut out);
        self.check_globals(&mut out);
        out
    }

    fn check_server(&self, out: &mut Vec<ConfigIssue>) {
        if self.routes.is_empty() {
            out.push(ConfigIssue::error(
                "no_routes",
                "route",
                "config must include at least one [[route]] block",
            ));
        }

        if let Some(tls) = &self.server.tls {
            let certs = tls.all_certs();
            if certs.is_empty() && !tls.acme.enabled {
                out.push(ConfigIssue::error(
                    "tls_without_cert",
                    "server.tls",
                    "[server.tls] is configured but has no certificate: add [[server.tls.cert]], \
                     cert_path/key_path, or enable [server.tls.acme]",
                ));
            }
            if tls.acme.enabled {
                if tls.acme.domains.is_empty() {
                    out.push(ConfigIssue::error(
                        "acme_without_domain",
                        "server.tls.acme.domains",
                        "[server.tls.acme] needs at least one domain",
                    ));
                }
                if tls.acme.directory_url.trim().is_empty() {
                    out.push(ConfigIssue::error(
                        "acme_directory_url_empty",
                        "server.tls.acme.directory_url",
                        "[server.tls.acme] directory_url must not be empty",
                    ));
                }
                if tls.acme.renew_before_days == 0 || tls.acme.renew_before_days > 89 {
                    out.push(ConfigIssue::error(
                        "acme_renew_window",
                        "server.tls.acme.renew_before_days",
                        "[server.tls.acme] renew_before_days must be between 1 and 89",
                    ));
                }
                for domain in &tls.acme.domains {
                    if domain.starts_with("*.") {
                        out.push(ConfigIssue::error(
                            "acme_wildcard_domain",
                            "server.tls.acme.domains",
                            format!(
                                "[server.tls.acme] cannot use the http-01 challenge for the \
                                 wildcard domain '{domain}'; wildcards need dns-01, which prx \
                                 does not support yet"
                            ),
                        ));
                    }
                }
            }
            if tls.cert_path.is_some() != tls.key_path.is_some() {
                out.push(ConfigIssue::error(
                    "tls_cert_key_pair",
                    "server.tls",
                    "[server.tls] cert_path and key_path must be set together",
                ));
            }
            if certs.iter().filter(|cert| cert.is_default).count() > 1 {
                out.push(ConfigIssue::error(
                    "tls_multiple_defaults",
                    "server.tls.cert",
                    "only one [[server.tls.cert]] can be marked is_default = true",
                ));
            }
            for cert in &certs {
                if cert.cert_path.trim().is_empty() || cert.key_path.trim().is_empty() {
                    out.push(ConfigIssue::error(
                        "tls_cert_incomplete",
                        "server.tls.cert",
                        "[[server.tls.cert]] needs both cert_path and key_path",
                    ));
                }
            }
        }

        if !self.server.health_path.starts_with('/') {
            out.push(ConfigIssue::error(
                "path_not_absolute",
                "server.health_path",
                "server.health_path must start with '/'",
            ));
        }
        if !self.server.ready_path.starts_with('/') {
            out.push(ConfigIssue::error(
                "path_not_absolute",
                "server.ready_path",
                "server.ready_path must start with '/'",
            ));
        }
        if self.server.health_path == self.server.ready_path {
            out.push(ConfigIssue::error(
                "health_ready_collision",
                "server.ready_path",
                "server.health_path and server.ready_path must be different",
            ));
        }
    }

    /// Returns the service names, which routes are checked against.
    fn check_services(&self, out: &mut Vec<ConfigIssue>) -> std::collections::HashSet<String> {
        let mut service_names = std::collections::HashSet::new();

        for (index, service) in self.services.iter().enumerate() {
            let at = |field: &str| format!("service[{index}].{field}");
            let name = &service.name;

            if service_names.contains(&service.name) {
                out.push(ConfigIssue::error(
                    "duplicate_service_name",
                    at("name"),
                    format!("duplicate service name '{name}'"),
                ));
            }
            service_names.insert(service.name.clone());

            if service.upstreams.is_empty() {
                out.push(ConfigIssue::error(
                    "service_without_upstream",
                    format!("service[{index}]"),
                    format!("service '{name}' must include at least one [[service.upstream]]"),
                ));
            }

            let mut seen_addrs: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for (uidx, upstream) in service.upstreams.iter().enumerate() {
                let upstream_at =
                    |field: &str| format!("service[{index}].upstream[{uidx}].{field}");
                if upstream.addr.trim().is_empty() {
                    out.push(ConfigIssue::error(
                        "upstream_addr_empty",
                        upstream_at("addr"),
                        format!("service '{name}' includes upstream with empty addr"),
                    ));
                } else if !seen_addrs.insert(upstream.addr.as_str()) {
                    out.push(ConfigIssue::warning(
                        "duplicate_upstream",
                        upstream_at("addr"),
                        format!(
                            "service '{name}' lists '{}' more than once, which only doubles the \
                             traffic it gets",
                            upstream.addr
                        ),
                    ));
                }

                // TLS that does not verify anything is a connection an attacker
                // on the path can take over, and nothing in the logs says so.
                if upstream.tls && upstream.verify_cert == Some(false) {
                    out.push(ConfigIssue::warning(
                        "upstream_tls_unverified",
                        upstream_at("verify_cert"),
                        format!(
                            "service '{name}' talks TLS to '{}' without verifying its \
                             certificate",
                            upstream.addr
                        ),
                    ));
                }

                for (field, value) in [
                    ("connect_timeout_ms", upstream.connect_timeout_ms),
                    (
                        "total_connect_timeout_ms",
                        upstream.total_connect_timeout_ms,
                    ),
                    ("read_timeout_ms", upstream.read_timeout_ms),
                    ("write_timeout_ms", upstream.write_timeout_ms),
                    ("idle_timeout_ms", upstream.idle_timeout_ms),
                ] {
                    if let Some(ms) = value.filter(|ms| *ms > HIGH_TIMEOUT_MS) {
                        out.push(timeout_warning(&name, upstream_at(field), field, ms));
                    }
                }
            }

            if service.request_timeout_ms > HIGH_TIMEOUT_MS {
                out.push(timeout_warning(
                    name,
                    at("request_timeout_ms"),
                    "request_timeout_ms",
                    service.request_timeout_ms,
                ));
            }

            if !service.upstreams.is_empty()
                && service.upstreams.iter().all(|upstream| !upstream.enabled)
            {
                out.push(ConfigIssue::warning(
                    "service_fully_drained",
                    format!("service[{index}]"),
                    format!(
                        "every upstream of service '{name}' is drained, so every request to it \
                         fails"
                    ),
                ));
            }

            if !(0.0..=10.0).contains(&service.retry_budget_ratio)
                || service.retry_budget_ratio.is_nan()
            {
                out.push(ConfigIssue::error(
                    "retry_budget_ratio_range",
                    at("retry_budget_ratio"),
                    format!("service '{name}' retry_budget_ratio must be between 0.0 and 10.0"),
                ));
            }
            if service.retry_budget_window_ms == 0 {
                out.push(ConfigIssue::error(
                    "retry_budget_window_zero",
                    at("retry_budget_window_ms"),
                    format!("service '{name}' retry_budget_window_ms must be > 0"),
                ));
            }

            if service.sticky.enabled {
                if service.sticky.name.trim().is_empty() {
                    out.push(ConfigIssue::error(
                        "sticky_name_empty",
                        at("sticky.name"),
                        format!("service '{name}' sticky.name must not be empty"),
                    ));
                }
                if service.sticky.mode == StickyMode::Header
                    && http::header::HeaderName::from_bytes(service.sticky.name.as_bytes()).is_err()
                {
                    out.push(ConfigIssue::error(
                        "sticky_name_invalid",
                        at("sticky.name"),
                        format!(
                            "service '{name}' sticky.name '{}' is not a valid header name",
                            service.sticky.name
                        ),
                    ));
                }
            }

            if service.health_check.enabled {
                let hc = &service.health_check;
                if hc.interval_ms == 0 {
                    out.push(ConfigIssue::error(
                        "health_check_interval_zero",
                        at("health_check.interval_ms"),
                        format!("service '{name}' health_check.interval_ms must be > 0"),
                    ));
                }
                if hc.timeout_ms == 0 {
                    out.push(ConfigIssue::error(
                        "health_check_timeout_zero",
                        at("health_check.timeout_ms"),
                        format!("service '{name}' health_check.timeout_ms must be > 0"),
                    ));
                }
                if hc.healthy_threshold == 0 || hc.unhealthy_threshold == 0 {
                    out.push(ConfigIssue::error(
                        "health_check_threshold_zero",
                        at("health_check.healthy_threshold"),
                        format!("service '{name}' health_check thresholds must be > 0"),
                    ));
                }
                if hc.timeout_ms >= hc.interval_ms && hc.interval_ms > 0 {
                    out.push(ConfigIssue::warning(
                        "health_check_timeout_over_interval",
                        at("health_check.timeout_ms"),
                        format!(
                            "service '{name}' health_check.timeout_ms ({}) is not shorter than \
                             interval_ms ({}), so probes overlap",
                            hc.timeout_ms, hc.interval_ms
                        ),
                    ));
                }
                if hc.kind == HealthCheckKind::Http {
                    if !hc.path.starts_with('/') {
                        out.push(ConfigIssue::error(
                            "path_not_absolute",
                            at("health_check.path"),
                            format!("service '{name}' health_check.path must start with '/'"),
                        ));
                    }
                    if hc.expected_status.is_empty() {
                        out.push(ConfigIssue::error(
                            "health_check_status_empty",
                            at("health_check.expected_status"),
                            format!(
                                "service '{name}' health_check.expected_status must not be empty"
                            ),
                        ));
                    }
                }
            }

            if service.circuit_breaker.enabled {
                if service.circuit_breaker.consecutive_failures == 0 {
                    out.push(ConfigIssue::error(
                        "circuit_breaker_failures_zero",
                        at("circuit_breaker.consecutive_failures"),
                        format!(
                            "service '{name}' circuit_breaker.consecutive_failures must be > 0"
                        ),
                    ));
                }
                if service.circuit_breaker.open_ms == 0 {
                    out.push(ConfigIssue::error(
                        "circuit_breaker_open_zero",
                        at("circuit_breaker.open_ms"),
                        format!("service '{name}' circuit_breaker.open_ms must be > 0"),
                    ));
                }
            }
        }

        service_names
    }

    fn check_routes(
        &self,
        service_names: &std::collections::HashSet<String>,
        out: &mut Vec<ConfigIssue>,
    ) {
        let mut defaults = 0usize;
        let mut used_services: std::collections::HashSet<&str> = std::collections::HashSet::new();
        // (host, path_prefix) already claimed, and by which route: a tie on
        // both is settled by file order, so the second one never matches.
        let mut claimed: std::collections::HashMap<(String, &str), &str> =
            std::collections::HashMap::new();

        for (index, route) in self.routes.iter().enumerate() {
            let at = |field: &str| format!("route[{index}].{field}");
            let name = &route.name;

            if route.is_default {
                defaults += 1;
            }
            used_services.insert(route.service.as_str());

            if route.path_prefix.is_empty() {
                out.push(ConfigIssue::error(
                    "path_prefix_empty",
                    at("path_prefix"),
                    format!("route '{name}' has empty path_prefix"),
                ));
            } else if !route.path_prefix.starts_with('/') {
                out.push(ConfigIssue::error(
                    "path_not_absolute",
                    at("path_prefix"),
                    format!("route '{name}' path_prefix must start with '/'"),
                ));
            }

            if !service_names.contains(&route.service) {
                let mut known: Vec<&str> = self.services.iter().map(|s| s.name.as_str()).collect();
                known.sort_unstable();
                out.push(
                    ConfigIssue::error(
                        "unknown_service",
                        at("service"),
                        format!(
                            "route '{name}' references unknown service '{}'",
                            route.service
                        ),
                    )
                    .with_hint(if known.is_empty() {
                        "no [[service]] is defined yet".to_string()
                    } else {
                        format!("services in this config: {}", known.join(", "))
                    }),
                );
            }

            if route.enabled {
                let host = route.host.clone().unwrap_or_default().to_ascii_lowercase();
                let key = (host, route.path_prefix.as_str());
                match claimed.get(&key) {
                    Some(first) if route.methods.is_empty() => {
                        out.push(ConfigIssue::warning(
                            "unreachable_route",
                            format!("route[{index}]"),
                            format!(
                                "route '{name}' can never match: route '{first}' claims the same \
                                 host and path_prefix, and a tie is settled by file order"
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        claimed.insert(key, name.as_str());
                    }
                }
            } else {
                out.push(ConfigIssue::warning(
                    "route_disabled",
                    format!("route[{index}]"),
                    format!("route '{name}' is disabled, so it takes no traffic at all"),
                ));
            }

            if route.cache.enabled {
                if route.cache.ttl_ms == 0 {
                    out.push(ConfigIssue::error(
                        "cache_ttl_zero",
                        at("cache.ttl_ms"),
                        format!("route '{name}' cache.ttl_ms must be > 0"),
                    ));
                }
                if route.cache.max_body_bytes == 0 {
                    out.push(ConfigIssue::error(
                        "cache_body_zero",
                        at("cache.max_body_bytes"),
                        format!("route '{name}' cache.max_body_bytes must be > 0"),
                    ));
                }
                if route.cache.cache_status_codes.is_empty() {
                    out.push(ConfigIssue::error(
                        "cache_status_empty",
                        at("cache.cache_status_codes"),
                        format!("route '{name}' cache.cache_status_codes must not be empty"),
                    ));
                }
                for header in &route.cache.vary_headers {
                    if http::header::HeaderName::from_bytes(header.as_bytes()).is_err() {
                        out.push(ConfigIssue::error(
                            "invalid_header_name",
                            at("cache.vary_headers"),
                            format!(
                                "route '{name}' cache.vary_headers contains an invalid header \
                                 name '{header}'"
                            ),
                        ));
                    }
                }
            }

            if route.rate_limit.enabled {
                if RateLimitKey::parse(&route.rate_limit.key).is_none() {
                    out.push(ConfigIssue::error(
                        "rate_limit_key_invalid",
                        at("rate_limit.key"),
                        format!(
                            "route '{name}' rate_limit.key '{}' is invalid (expected \
                             'client_ip', 'route', or 'header:<Name>')",
                            route.rate_limit.key
                        ),
                    ));
                }
                if route.rate_limit.requests_per_second == 0 {
                    out.push(ConfigIssue::error(
                        "rate_limit_rps_zero",
                        at("rate_limit.requests_per_second"),
                        format!("route '{name}' rate_limit.requests_per_second must be > 0"),
                    ));
                }
                if !(400..=599).contains(&route.rate_limit.response_status) {
                    out.push(ConfigIssue::error(
                        "response_status_range",
                        at("rate_limit.response_status"),
                        format!(
                            "route '{name}' rate_limit.response_status must be a 4xx or 5xx code"
                        ),
                    ));
                }
                if route.rate_limit.max_entries == 0 {
                    out.push(ConfigIssue::error(
                        "rate_limit_entries_zero",
                        at("rate_limit.max_entries"),
                        format!("route '{name}' rate_limit.max_entries must be > 0"),
                    ));
                }
            }

            if !(400..=599).contains(&route.concurrency_limit.response_status) {
                out.push(ConfigIssue::error(
                    "response_status_range",
                    at("concurrency_limit.response_status"),
                    format!(
                        "route '{name}' concurrency_limit.response_status must be a 4xx or 5xx code"
                    ),
                ));
            }

            check_header_rules(
                &route.request_headers,
                &format!("route '{name}'"),
                &at("request_headers"),
                out,
            );
            check_header_rules(
                &route.response_headers,
                &format!("route '{name}'"),
                &at("response_headers"),
                out,
            );

            // An unknown method would silently make the route unreachable, so
            // reject it while the config is being loaded instead.
            for method in &route.methods {
                if crate::router::method_bit(method).is_none() {
                    out.push(ConfigIssue::error(
                        "unknown_method",
                        at("methods"),
                        format!(
                            "route '{name}' lists unsupported HTTP method '{method}' \
                             (supported: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE, \
                             CONNECT)"
                        ),
                    ));
                }
            }
        }

        if defaults > 1 {
            out.push(ConfigIssue::error(
                "multiple_default_routes",
                "route",
                "only one route can be marked is_default = true",
            ));
        }
        if defaults == 0 && !self.routes.is_empty() {
            out.push(ConfigIssue::warning(
                "no_default_route",
                "route",
                "no route is marked is_default = true, so anything that matches nothing gets a \
                 404",
            ));
        }

        for (index, service) in self.services.iter().enumerate() {
            if !used_services.contains(service.name.as_str()) {
                out.push(ConfigIssue::warning(
                    "unused_service",
                    format!("service[{index}]"),
                    format!("service '{}' is not used by any route", service.name),
                ));
            }
        }
    }

    /// Plugin blocks and the routes that reference them (T501).
    ///
    /// Everything here is checked without building anything: an unknown kind or
    /// a bad setting is a config error with a path the editor can point at,
    /// not a panic at boot.
    fn check_plugins(&self, out: &mut Vec<ConfigIssue>) {
        let mut declared: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        for (index, plugin) in self.plugins.iter().enumerate() {
            let at = |field: &str| format!("plugin[{index}].{field}");

            if plugin.name.trim().is_empty() {
                out.push(ConfigIssue::error(
                    "plugin_name_empty",
                    at("name"),
                    "every [[plugin]] needs a name for routes to refer to",
                ));
            }

            if let Some(first) = declared.insert(plugin.name.as_str(), index) {
                out.push(
                    ConfigIssue::error(
                        "duplicate_plugin",
                        at("name"),
                        format!("plugin '{}' is declared more than once", plugin.name),
                    )
                    .with_hint(format!("first declared as plugin[{first}]")),
                );
            }

            if let Some(problem) = crate::plugin::check_plugin_config(&plugin.kind, &plugin.config)
            {
                // An unknown kind is about `kind`; a bad setting is about the
                // settings table. Pointing at the right one is the difference
                // between the editor highlighting the mistake and highlighting
                // its neighbour.
                let known = crate::plugin::known_kinds().contains(&plugin.kind.as_str());
                let (code, path) = if known {
                    ("plugin_config_invalid", at("config"))
                } else {
                    ("unknown_plugin_kind", at("kind"))
                };
                out.push(
                    ConfigIssue::error(code, path, format!("plugin '{}': {problem}", plugin.name))
                        .with_hint(format!(
                            "plugin kinds in this build: {}",
                            crate::plugin::known_kinds().join(", ")
                        )),
                );
            }
        }

        let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (index, route) in self.routes.iter().enumerate() {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for (slot, wanted) in route.plugins.iter().enumerate() {
                used.insert(wanted.as_str());

                if !declared.contains_key(wanted.as_str()) {
                    let mut known: Vec<&str> =
                        self.plugins.iter().map(|p| p.name.as_str()).collect();
                    known.sort_unstable();
                    out.push(
                        ConfigIssue::error(
                            "unknown_plugin",
                            format!("route[{index}].plugins[{slot}]"),
                            format!(
                                "route '{}' references unknown plugin '{wanted}'",
                                route.name
                            ),
                        )
                        .with_hint(if known.is_empty() {
                            "no [[plugin]] is defined yet".to_string()
                        } else {
                            format!("plugins in this config: {}", known.join(", "))
                        }),
                    );
                } else if !seen.insert(wanted.as_str()) {
                    // Running the same plugin twice on one route is far more
                    // likely to be a copy-paste slip than a plan.
                    out.push(ConfigIssue::warning(
                        "duplicate_route_plugin",
                        format!("route[{index}].plugins[{slot}]"),
                        format!(
                            "route '{}' lists plugin '{wanted}' more than once",
                            route.name
                        ),
                    ));
                }
            }
        }

        for (index, plugin) in self.plugins.iter().enumerate() {
            if plugin.enabled && !used.contains(plugin.name.as_str()) {
                out.push(
                    ConfigIssue::warning(
                        "unused_plugin",
                        format!("plugin[{index}].name"),
                        format!("plugin '{}' is not used by any route", plugin.name),
                    )
                    .with_hint("add it to a route's `plugins` list, or remove the block"),
                );
            }
        }
    }

    fn check_admin(&self, out: &mut Vec<ConfigIssue>) {
        if self.admin.auth == AdminAuth::None && !is_loopback_listen(&self.admin.listen) {
            out.push(
                ConfigIssue::error(
                    "admin_open_on_public_listen",
                    "admin.listen",
                    "admin.auth is none but admin.listen is not a loopback address; set auth to token or basic, or bind the admin API to 127.0.0.1 or ::1",
                )
                .with_hint(
                    "a control plane that can rewrite the proxy config must not be reachable without a credential",
                ),
            );
        }
        for (index, origin) in self.admin.allow_origins.iter().enumerate() {
            let trimmed = origin.trim();
            if trimmed == "*" || trimmed.is_empty() || !origin_is_explicit(trimmed) {
                out.push(ConfigIssue::error(
                    "admin_origin_not_explicit",
                    format!("admin.allow_origins[{index}]"),
                    format!(
                        "admin.allow_origins[{index}] must be an explicit origin such as https://ops.example.com; '*' and bare hosts are not allowed"
                    ),
                ));
            }
        }
    }

    /// Loads the rules file and reports what the loader found: a missing or
    /// unreadable file, or a rule that did not compile, is an error here.
    fn check_waf(&self, out: &mut Vec<ConfigIssue>) {
        let Some(path) = self.waf.resolved_rules_file() else {
            return;
        };
        for issue in crate::waf::load(&[path]).issues {
            let location = match issue.rule_id {
                Some(id) => format!("{}:{} [rule {id}]", issue.file, issue.line),
                None => format!("{}:{}", issue.file, issue.line),
            };
            let message = format!("{location} {}", issue.message);
            out.push(match issue.severity {
                IssueSeverity::Error => ConfigIssue::error(issue.code, "waf.rules_file", message),
                IssueSeverity::Warning => {
                    ConfigIssue::warning(issue.code, "waf.rules_file", message)
                }
            });
        }
    }

    /// The optional paranoia level and anomaly threshold. They are checked
    /// whether or not a rules file is set.
    fn check_waf_tuning(&self, out: &mut Vec<ConfigIssue>) {
        if let Some(level) = self.waf.paranoia_level
            && !(1..=4).contains(&level)
        {
            out.push(ConfigIssue::error(
                "paranoia_level_range",
                "waf.paranoia_level",
                format!("waf.paranoia_level must be between 1 and 4, got {level}"),
            ));
        }
        if let Some(threshold) = self.waf.inbound_anomaly_threshold
            && threshold < 1
        {
            out.push(ConfigIssue::error(
                "inbound_anomaly_threshold_range",
                "waf.inbound_anomaly_threshold",
                format!(
                    "waf.inbound_anomaly_threshold must be a positive integer, got {threshold}"
                ),
            ));
        }
    }

    fn check_globals(&self, out: &mut Vec<ConfigIssue>) {
        if self.compression.enabled && !(1..=11).contains(&self.compression.level) {
            out.push(ConfigIssue::error(
                "compression_level_range",
                "compression.level",
                "compression.level must be between 1 and 11",
            ));
        }

        check_header_rules(
            &self.headers.request,
            "headers.request",
            "headers.request",
            out,
        );
        check_header_rules(
            &self.headers.response,
            "headers.response",
            "headers.response",
            out,
        );
    }
}

/// How much a [`ConfigIssue`] matters: an error blocks the apply, a warning
/// does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warning,
}

/// Above five minutes a timeout stops being a safety net: a stuck upstream holds
/// the connection, and the worker behind it, for longer than any client waits.
const HIGH_TIMEOUT_MS: u64 = 300_000;

fn timeout_warning(service: &str, path: String, field: &str, ms: u64) -> ConfigIssue {
    ConfigIssue::warning(
        "timeout_unusually_high",
        path,
        format!(
            "service '{service}' {field} = {ms} is over {HIGH_TIMEOUT_MS} ms, so a stuck \
             upstream holds its connection for minutes"
        ),
    )
}

/// One problem found in a config, addressed by field path so the editor can
/// turn it into a mark on the right line (T203).
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ConfigIssue {
    pub severity: IssueSeverity,
    /// Stable identifier, so the UI can translate the message.
    pub code: &'static str,
    /// Field path, e.g. `route[2].service`.
    pub path: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ConfigIssue {
    pub fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: IssueSeverity::Error,
            code,
            path: path.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn warning(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: IssueSeverity::Warning,
            code,
            path: path.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
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

/// Header rules are checked by hand rather than by serde: an invalid header
/// name only shows up when the proxy tries to send it, which is far too late.
fn check_header_rules(rules: &HeaderRules, context: &str, path: &str, out: &mut Vec<ConfigIssue>) {
    let check_name = |name: &str, field: &str, out: &mut Vec<ConfigIssue>| {
        if http::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            out.push(ConfigIssue::error(
                "invalid_header_name",
                format!("{path}.{field}"),
                format!("{context} has an invalid header name '{name}'"),
            ));
        }
    };

    for (kind, map) in [("set", &rules.set), ("add", &rules.add)] {
        for (name, value) in map.iter() {
            check_name(name, kind, out);
            for variable in extract_variables(value) {
                if !HEADER_VARIABLES.contains(&variable.as_str()) {
                    out.push(
                        ConfigIssue::error(
                            "unknown_header_variable",
                            format!("{path}.{kind}"),
                            format!(
                                "{context} header '{name}' uses unknown variable '${variable}'"
                            ),
                        )
                        .with_hint(format!(
                            "supported: {}",
                            HEADER_VARIABLES
                                .iter()
                                .map(|v| format!("${v}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    );
                }
            }
        }
    }
    for name in &rules.remove {
        check_name(name, "remove", out);
    }
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

/// How the admin API checks a caller.
///
/// `none` is only legal on a loopback listener: a control plane that can
/// rewrite the proxy's config must not be reachable without a credential.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum AdminAuth {
    #[default]
    None,
    Token,
    Basic,
}

/// The `[admin]` block.
///
/// The credential itself never lives here. `token_file` and `basic_file`
/// point at a file (or, when unset, the `PRX_ADMIN_TOKEN` / `PRX_ADMIN_BASIC`
/// environment variables), so a config that is shown, diffed or committed
/// cannot carry the secret.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AdminConfig {
    #[serde(default = "default_admin_listen")]
    pub listen: String,
    #[serde(default)]
    pub auth: AdminAuth,
    /// Path to a file holding the bearer token, one line. Ignored unless
    /// `auth` is `token`.
    #[serde(default)]
    pub token_file: Option<String>,
    /// Path to a file holding `user:password`, one line. Ignored unless
    /// `auth` is `basic`.
    #[serde(default)]
    pub basic_file: Option<String>,
    /// Origins allowed to call the API from another site. Empty means
    /// same-origin only; `*` is never valid.
    #[serde(default)]
    pub allow_origins: Vec<String>,
    /// Serve the API for reading and refuse every change.
    #[serde(default)]
    pub read_only: bool,
}

fn default_admin_listen() -> String {
    "127.0.0.1:9091".to_string()
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            listen: default_admin_listen(),
            auth: AdminAuth::None,
            token_file: None,
            basic_file: None,
            allow_origins: Vec::new(),
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct TlsConfig {
    pub listen: String,
    /// Single-certificate form, kept so existing configs keep working. Prefer
    /// `[[server.tls.cert]]`, which supports more than one domain.
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default = "default_true")]
    pub enable_h2: bool,
    /// Certificates to choose from by SNI.
    #[serde(rename = "cert", default)]
    pub certs: Vec<TlsCertConfig>,
    /// Obtain and renew certificates automatically.
    #[serde(default)]
    pub acme: AcmeConfig,
}

/// Automatic certificates over ACME (Let's Encrypt and compatible servers).
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AcmeConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Contact addresses registered with the ACME account.
    #[serde(default)]
    pub email: Vec<String>,
    #[serde(default = "default_acme_directory")]
    pub directory_url: String,
    /// Domains to request a certificate for.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Where the account key and issued certificate are kept.
    #[serde(default = "default_acme_storage")]
    pub storage_dir: String,
    /// Renew this many days before expiry.
    #[serde(default = "default_renew_before_days")]
    pub renew_before_days: u32,
    /// PEM file with the CA that signs the ACME server's own TLS certificate.
    /// Needed for a private ACME server (step-ca, Pebble); public providers
    /// are trusted through the system roots.
    #[serde(default)]
    pub ca_root_path: Option<String>,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            email: Vec::new(),
            directory_url: default_acme_directory(),
            domains: Vec::new(),
            storage_dir: default_acme_storage(),
            renew_before_days: default_renew_before_days(),
            ca_root_path: None,
        }
    }
}

fn default_acme_directory() -> String {
    // Staging by default: the production endpoint has strict rate limits, and
    // a misconfigured deployment should not burn through them.
    "https://acme-staging-v02.api.letsencrypt.org/directory".to_string()
}

fn default_acme_storage() -> String {
    "./acme".to_string()
}

fn default_renew_before_days() -> u32 {
    30
}

impl TlsConfig {
    /// All certificates in one list, whichever form the config used.
    pub fn all_certs(&self) -> Vec<TlsCertConfig> {
        let mut certs = self.certs.clone();
        if let (Some(cert_path), Some(key_path)) = (&self.cert_path, &self.key_path) {
            certs.push(TlsCertConfig {
                domains: Vec::new(),
                cert_path: cert_path.clone(),
                key_path: key_path.clone(),
                is_default: certs.is_empty(),
            });
        }
        certs
    }
}

/// One certificate and the domains it answers for.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct TlsCertConfig {
    /// Domains this certificate serves. `*.example.com` also covers
    /// `example.com`. Left empty, the names are read from the certificate
    /// itself.
    #[serde(default)]
    pub domains: Vec<String>,
    pub cert_path: String,
    pub key_path: String,
    /// Used for clients that send no SNI, or a name no certificate covers.
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ServiceConfig {
    #[serde(default = "default_service_name")]
    pub name: String,
    #[serde(default)]
    pub lb: LbStrategy,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default)]
    pub retry_backoff_ms: u64,
    /// Fraction of recent successful requests that may be spent on retries.
    /// `0.0` disables the budget (retries are then limited only by
    /// `max_retries`). Keeps a failing upstream from being hammered by the
    /// retries of every in-flight request at once.
    #[serde(default = "default_retry_budget_ratio")]
    pub retry_budget_ratio: f64,
    /// Requests below this count per window may always retry, so a service
    /// that is idle or just started is not locked out by its own budget.
    #[serde(default = "default_retry_budget_min")]
    pub retry_budget_min_per_window: u64,
    /// Window the retry budget is measured over.
    #[serde(default = "default_retry_budget_window_ms")]
    pub retry_budget_window_ms: u64,
    /// Only retry requests whose method is idempotent once the request may
    /// already have reached the upstream. Connect failures are always
    /// retriable: nothing was sent yet.
    #[serde(default = "default_true")]
    pub retry_idempotent_only: bool,
    /// Total time budget for a request including every retry. `0` disables it.
    #[serde(default)]
    pub request_timeout_ms: u64,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    /// Probe upstreams in the background instead of waiting for a real request
    /// to discover that one is down.
    #[serde(default)]
    pub health_check: HealthCheckConfig,
    /// Keep a client on the same upstream across requests.
    #[serde(default)]
    pub sticky: StickyConfig,
    /// Which HTTP version to speak to the upstreams of this service.
    #[serde(default)]
    pub upstream_h2: UpstreamH2,
    #[serde(rename = "upstream", default)]
    pub upstreams: Vec<UpstreamConfig>,
}

/// Session affinity: keep a client on the upstream it used last.
///
/// `cookie` is the only mode that survives a client changing IP, and it is the
/// only one that pins exactly; `client_ip` and `header` hash the key onto the
/// upstream ring, so adding or removing an upstream reshuffles some clients.
/// Every mode falls back to normal load balancing when the chosen upstream is
/// unavailable, so affinity never costs availability.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct StickyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: StickyMode,
    /// Cookie or header name, depending on the mode.
    #[serde(default = "default_sticky_name")]
    pub name: String,
    /// Lifetime of the cookie in `cookie` mode.
    #[serde(default = "default_sticky_ttl_s")]
    pub ttl_s: u64,
}

impl Default for StickyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: StickyMode::default(),
            name: default_sticky_name(),
            ttl_s: default_sticky_ttl_s(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StickyMode {
    /// prx sets a cookie naming the upstream and honors it on later requests.
    #[default]
    Cookie,
    /// Hash the client address.
    ClientIp,
    /// Hash the value of a request header.
    Header,
}

fn default_sticky_name() -> String {
    "prx_upstream".to_string()
}

fn default_sticky_ttl_s() -> u64 {
    3_600
}

/// Background probing of a service's upstreams.
///
/// Passive detection (the circuit breaker) only removes an upstream after real
/// requests have already failed, so users see the errors. An active probe finds
/// the same failure before a request does, and proves an upstream is healthy
/// again before sending traffic back to it.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct HealthCheckConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: HealthCheckKind,
    /// Request path for `kind = "http"`.
    #[serde(default = "default_health_check_path")]
    pub path: String,
    #[serde(default = "default_health_check_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_health_check_timeout_ms")]
    pub timeout_ms: u64,
    /// Consecutive successes before a down upstream is used again.
    #[serde(default = "default_healthy_threshold")]
    pub healthy_threshold: u32,
    /// Consecutive failures before an upstream is taken out.
    #[serde(default = "default_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
    /// Response codes that count as healthy for `kind = "http"`.
    #[serde(default = "default_expected_status")]
    pub expected_status: Vec<u16>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: HealthCheckKind::default(),
            path: default_health_check_path(),
            interval_ms: default_health_check_interval_ms(),
            timeout_ms: default_health_check_timeout_ms(),
            healthy_threshold: default_healthy_threshold(),
            unhealthy_threshold: default_unhealthy_threshold(),
            expected_status: default_expected_status(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckKind {
    /// Open a TCP connection and close it again.
    #[default]
    Tcp,
    /// Send a GET and check the status code.
    Http,
}

fn default_health_check_path() -> String {
    "/healthz".to_string()
}

fn default_health_check_interval_ms() -> u64 {
    2_000
}

fn default_health_check_timeout_ms() -> u64 {
    1_000
}

fn default_healthy_threshold() -> u32 {
    2
}

fn default_unhealthy_threshold() -> u32 {
    3
}

fn default_expected_status() -> Vec<u16> {
    vec![200]
}

/// How prx talks to a service's upstreams.
///
/// gRPC needs `always`: it requires end-to-end HTTP/2 because it carries its
/// status in trailers, which HTTP/1.1 cannot express.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default, schemars::JsonSchema,
)]
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

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: default_service_name(),
            lb: LbStrategy::default(),
            upstream_h2: UpstreamH2::default(),
            max_retries: 0,
            retry_backoff_ms: 0,
            retry_budget_ratio: default_retry_budget_ratio(),
            retry_budget_min_per_window: default_retry_budget_min(),
            retry_budget_window_ms: default_retry_budget_window_ms(),
            retry_idempotent_only: true,
            request_timeout_ms: 0,
            circuit_breaker: CircuitBreakerConfig::default(),
            health_check: HealthCheckConfig::default(),
            sticky: StickyConfig::default(),
            upstreams: Vec::new(),
        }
    }
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            name: default_route_name(),
            service: String::new(),
            host: None,
            path_prefix: default_path_prefix(),
            methods: Vec::new(),
            is_default: false,
            // Never derive Default for this struct: `#[serde(default = ...)]`
            // only applies while parsing, so a derived one would build routes
            // that are off.
            enabled: true,
            request_headers: HeaderRules::default(),
            response_headers: HeaderRules::default(),
            rate_limit: RateLimitConfig::default(),
            concurrency_limit: ConcurrencyLimitConfig::default(),
            cache: CacheConfig::default(),
            plugins: Vec::new(),
        }
    }
}

fn default_retry_budget_ratio() -> f64 {
    0.2
}

fn default_retry_budget_min() -> u64 {
    10
}

fn default_retry_budget_window_ms() -> u64 {
    10_000
}

/// Header rules applied to a request or a response.
///
/// `set` replaces any existing value, `add` appends another value, and
/// `remove` drops the header. They are applied in that order, so a rule set
/// can normalize a header and then append to it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
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

/// Response compression.
///
/// prx uses pingora's streaming compressor, so a large response is compressed
/// as it flows through rather than being buffered first.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CompressionConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Compression level. Higher costs CPU for a smaller body; 3-6 is the
    /// usual range for a proxy that compresses on the fly.
    #[serde(default = "default_compression_level")]
    pub level: u32,
    /// Decompress an already-compressed upstream response when the client
    /// cannot accept that encoding. Off by default: it is expensive and rarely
    /// what an operator wants.
    #[serde(default)]
    pub decompress_upstream: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: default_compression_level(),
            decompress_upstream: false,
        }
    }
}

fn default_compression_level() -> u32 {
    4
}

/// Header rules that apply to every route, applied before the route's own.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct HeadersConfig {
    #[serde(default)]
    pub request: HeaderRules,
    #[serde(default)]
    pub response: HeaderRules,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
    /// A disabled route stays in the file but is left out of the route index,
    /// so it never matches — the way to park a route without losing how it was
    /// configured.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub request_headers: HeaderRules,
    #[serde(default)]
    pub response_headers: HeaderRules,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub concurrency_limit: ConcurrencyLimitConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    /// Plugins to run for this route, in this order (T501). Names refer to
    /// `[[plugin]]` blocks.
    #[serde(default)]
    pub plugins: Vec<String>,
}

/// Short-lived response cache for a route.
///
/// Even a one-second TTL turns a burst of identical requests into a single
/// upstream fetch, which is where most of the benefit comes from.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cache_ttl_ms")]
    pub ttl_ms: u64,
    /// Responses larger than this are streamed through without being stored.
    #[serde(default = "default_cache_max_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default = "default_cache_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_cache_max_bytes")]
    pub max_bytes: usize,
    /// Status codes worth storing.
    #[serde(default = "default_cache_status_codes")]
    pub cache_status_codes: Vec<u16>,
    /// Include the query string in the cache key.
    #[serde(default = "default_true")]
    pub key_query: bool,
    /// Request headers whose values take part in the cache key, so responses
    /// that differ by them are not mixed up.
    #[serde(default = "default_cache_vary")]
    pub vary_headers: Vec<String>,
    /// How long a coalesced request waits for the in-flight fetch before going
    /// upstream itself.
    #[serde(default = "default_cache_coalesce_wait_ms")]
    pub coalesce_wait_ms: u64,
    /// Add an `X-Cache: HIT|MISS` header to responses.
    #[serde(default = "default_true")]
    pub add_status_header: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_ms: default_cache_ttl_ms(),
            max_body_bytes: default_cache_max_body_bytes(),
            max_entries: default_cache_max_entries(),
            max_bytes: default_cache_max_bytes(),
            cache_status_codes: default_cache_status_codes(),
            key_query: true,
            vary_headers: default_cache_vary(),
            coalesce_wait_ms: default_cache_coalesce_wait_ms(),
            add_status_header: true,
        }
    }
}

fn default_cache_ttl_ms() -> u64 {
    2_000
}

fn default_cache_max_body_bytes() -> usize {
    256 * 1024
}

fn default_cache_max_entries() -> usize {
    10_000
}

fn default_cache_max_bytes() -> usize {
    128 * 1024 * 1024
}

fn default_cache_status_codes() -> Vec<u16> {
    vec![200, 203, 300, 301, 404]
}

fn default_cache_vary() -> Vec<String> {
    vec!["accept-encoding".to_string()]
}

fn default_cache_coalesce_wait_ms() -> u64 {
    2_000
}

/// Per-route rate limiting.
///
/// Cheap enough to leave on: one hash and a short lock on a sharded bucket per
/// request, with a hard cap on how many keys are tracked so a flood of unique
/// keys cannot grow memory without bound.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct RateLimitConfig {
    #[serde(default)]
    pub enabled: bool,
    /// What to count per: `client_ip`, `route`, or `header:<Name>`.
    #[serde(default = "default_rate_limit_key")]
    pub key: String,
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u64,
    /// How many requests may arrive at once before the sustained rate applies.
    #[serde(default)]
    pub burst: u64,
    #[serde(default = "default_rate_limit_status")]
    pub response_status: u16,
    /// Send a `Retry-After` header with the rejection.
    #[serde(default = "default_true")]
    pub retry_after: bool,
    /// Keys idle for this long may be forgotten.
    #[serde(default = "default_rate_limit_ttl_ms")]
    pub entry_ttl_ms: u64,
    /// Upper bound on tracked keys.
    #[serde(default = "default_rate_limit_max_entries")]
    pub max_entries: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key: default_rate_limit_key(),
            requests_per_second: default_requests_per_second(),
            burst: 0,
            response_status: default_rate_limit_status(),
            retry_after: true,
            entry_ttl_ms: default_rate_limit_ttl_ms(),
            max_entries: default_rate_limit_max_entries(),
        }
    }
}

/// Caps how many requests a route may have in flight at once.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ConcurrencyLimitConfig {
    /// `0` means unlimited.
    #[serde(default)]
    pub max_concurrent: usize,
    #[serde(default = "default_concurrency_status")]
    pub response_status: u16,
}

impl Default for ConcurrencyLimitConfig {
    /// Never derive this: `#[serde(default = ...)]` only applies when parsing,
    /// so a derived Default would leave `response_status` at 0 and fail the
    /// config's own validation for every config built in Rust.
    fn default() -> Self {
        Self {
            max_concurrent: 0,
            response_status: default_concurrency_status(),
        }
    }
}

fn default_rate_limit_key() -> String {
    "client_ip".to_string()
}

fn default_requests_per_second() -> u64 {
    100
}

fn default_rate_limit_status() -> u16 {
    429
}

fn default_rate_limit_ttl_ms() -> u64 {
    60_000
}

fn default_rate_limit_max_entries() -> usize {
    100_000
}

fn default_concurrency_status() -> u16 {
    503
}

/// How a rate limit counts requests.
#[derive(Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
pub enum RateLimitKey {
    /// One bucket per client address.
    ClientIp,
    /// One bucket for the whole route.
    Route,
    /// One bucket per distinct value of a header.
    Header(String),
}

impl RateLimitKey {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if let Some(name) = raw.strip_prefix("header:") {
            let name = name.trim();
            if name.is_empty() || http::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
                return None;
            }
            return Some(Self::Header(name.to_ascii_lowercase()));
        }
        match raw {
            "client_ip" => Some(Self::ClientIp),
            "route" => Some(Self::Route),
            _ => None,
        }
    }
}

fn default_route_name() -> String {
    "default".to_string()
}

fn default_path_prefix() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LbStrategy {
    #[default]
    RoundRobin,
    Random,
    Hash,
    /// Send the request to whichever of two random upstreams has fewer
    /// requests in flight. Power of two choices avoids scanning every
    /// upstream while getting most of the benefit of true least-connections.
    LeastConn,
    /// Like `least_conn`, but weighted by a moving average of each upstream's
    /// latency, so a slow-but-accepting upstream is avoided too.
    P2cEwma,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
            "least_conn" => Ok(LbStrategy::LeastConn),
            "p2c_ewma" => Ok(LbStrategy::P2cEwma),
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

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpstreamConfig {
    pub addr: String,
    /// `false` drains the upstream: it stays in the config, keeps being probed,
    /// and is left out of the selection ring so it takes no traffic.
    ///
    /// This cannot be expressed as `weight = 0` — the balancer clamps weights to
    /// at least 1, so a zero-weight upstream would quietly keep serving.
    #[serde(default = "default_true")]
    pub enabled: bool,
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

/// An origin the admin API may answer to: scheme, host and an optional port,
/// nothing else. `*` is deliberately not an origin.
fn origin_is_explicit(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    if scheme != "http" && scheme != "https" {
        return false;
    }
    let host_port = rest.split(['/', '?', '#', ' ']).next().unwrap_or("");
    if host_port.is_empty() || host_port != rest {
        return false;
    }
    let host = host_port
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(host_port);
    !host.is_empty() && !host.contains('*')
}

/// Whether an admin listen address can only be reached from the same machine.
///
/// Anything that does not parse, or parses as a wildcard, is treated as
/// public: the safe failure is to demand a credential.
pub fn is_loopback_listen(listen: &str) -> bool {
    let Ok(addr) = listen.parse::<std::net::SocketAddr>() else {
        return false;
    };
    addr.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_upstream(addr: &str) -> UpstreamConfig {
        UpstreamConfig {
            addr: addr.to_string(),
            enabled: true,
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
            ..Default::default()
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
            ..Default::default()
        }
    }

    fn valid_config() -> PrxConfig {
        PrxConfig {
            server: ServerConfig::default(),
            admin: Default::default(),
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
            compression: Default::default(),
            services: vec![valid_service("default")],
            routes: vec![valid_route("default", "default")],
            plugins: Vec::new(),
            waf: Default::default(),
        }
    }

    // --- plugins (T501) ----------------------------------------------------

    fn plugin_block(name: &str, kind: &str, settings: toml::Value) -> PluginConfig {
        PluginConfig {
            name: name.to_string(),
            kind: kind.to_string(),
            enabled: true,
            config: settings,
        }
    }

    fn echo_settings() -> toml::Value {
        let mut table = toml::map::Map::new();
        table.insert(
            "request_header".into(),
            toml::Value::String("x-mark".into()),
        );
        table.insert("value".into(), toml::Value::String("one".into()));
        toml::Value::Table(table)
    }

    fn codes(issues: &[ConfigIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn a_config_with_no_plugins_reports_nothing_about_them() {
        let issues = valid_config().check();
        assert!(
            !codes(&issues).iter().any(|code| code.contains("plugin")),
            "an untouched config grew a plugin complaint: {issues:?}"
        );
    }

    #[test]
    fn a_route_can_use_a_declared_plugin() {
        let mut cfg = valid_config();
        cfg.plugins
            .push(plugin_block("mark", "echo-header", echo_settings()));
        cfg.routes[0].plugins = vec!["mark".to_string()];
        let issues = cfg.check();
        assert!(
            issues
                .iter()
                .all(|issue| issue.severity != IssueSeverity::Error),
            "a valid plugin config was rejected: {issues:?}"
        );
    }

    #[test]
    fn an_unknown_plugin_kind_is_an_error_at_its_own_field() {
        let mut cfg = valid_config();
        cfg.plugins
            .push(plugin_block("mark", "not-a-real-kind", echo_settings()));
        cfg.routes[0].plugins = vec!["mark".to_string()];
        let issue = cfg
            .check()
            .into_iter()
            .find(|issue| issue.code == "unknown_plugin_kind")
            .expect("an unknown kind should be reported");
        // The path is what the editor highlights; pointing at the block
        // instead of the field is the difference between a useful error and a
        // vague one.
        assert_eq!(issue.path, "plugin[0].kind");
        assert!(issue.hint.is_some_and(|hint| hint.contains("echo-header")));
    }

    #[test]
    fn a_bad_setting_is_reported_against_the_settings_table() {
        let mut cfg = valid_config();
        // `value` is required, and nothing else is set either.
        cfg.plugins.push(plugin_block(
            "mark",
            "echo-header",
            toml::Value::Table(toml::map::Map::new()),
        ));
        cfg.routes[0].plugins = vec!["mark".to_string()];
        let issue = cfg
            .check()
            .into_iter()
            .find(|issue| issue.code == "plugin_config_invalid")
            .expect("a missing required setting should be reported");
        assert_eq!(issue.path, "plugin[0].config");
    }

    #[test]
    fn a_route_referring_to_a_plugin_that_does_not_exist_is_an_error() {
        let mut cfg = valid_config();
        cfg.routes[0].plugins = vec!["ghost".to_string()];
        let issue = cfg
            .check()
            .into_iter()
            .find(|issue| issue.code == "unknown_plugin")
            .expect("an unknown plugin name should be reported");
        assert_eq!(issue.path, "route[0].plugins[0]");
    }

    #[test]
    fn declaring_the_same_plugin_twice_is_an_error() {
        let mut cfg = valid_config();
        cfg.plugins
            .push(plugin_block("mark", "echo-header", echo_settings()));
        cfg.plugins
            .push(plugin_block("mark", "echo-header", echo_settings()));
        cfg.routes[0].plugins = vec!["mark".to_string()];
        assert!(codes(&cfg.check()).contains(&"duplicate_plugin"));
    }

    #[test]
    fn a_plugin_no_route_uses_is_a_warning_not_an_error() {
        let mut cfg = valid_config();
        cfg.plugins
            .push(plugin_block("mark", "echo-header", echo_settings()));
        let issue = cfg
            .check()
            .into_iter()
            .find(|issue| issue.code == "unused_plugin")
            .expect("an unused plugin should be mentioned");
        // A plugin waiting to be wired up is a work in progress, not a broken
        // config: warning, so the file still applies.
        assert_eq!(issue.severity, IssueSeverity::Warning);
    }

    #[test]
    fn listing_a_plugin_twice_on_one_route_is_a_warning() {
        let mut cfg = valid_config();
        cfg.plugins
            .push(plugin_block("mark", "echo-header", echo_settings()));
        cfg.routes[0].plugins = vec!["mark".to_string(), "mark".to_string()];
        let issue = cfg
            .check()
            .into_iter()
            .find(|issue| issue.code == "duplicate_route_plugin")
            .expect("a repeated plugin should be mentioned");
        assert_eq!(issue.severity, IssueSeverity::Warning);
    }

    #[test]
    fn plugins_survive_a_round_trip_through_toml() {
        let text = r#"
[[plugin]]
name = "mark"
kind = "echo-header"

[plugin.config]
request_header = "x-mark"
value = "one"

[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3000"

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
plugins = ["mark"]
"#;
        let cfg = PrxConfig::from_toml_str(text).expect("config should parse and validate");
        assert_eq!(cfg.plugins.len(), 1);
        assert_eq!(cfg.plugins[0].kind, "echo-header");
        assert!(cfg.plugins[0].enabled, "plugins are on unless turned off");
        assert_eq!(cfg.routes[0].plugins, vec!["mark".to_string()]);
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

    #[test]
    fn admin_none_auth_is_refused_on_a_public_listen() {
        let mut config = valid_config();
        config.admin.listen = "0.0.0.0:9091".into();
        config.admin.auth = AdminAuth::None;
        let codes = codes(&config.check());
        assert!(codes.contains(&"admin_open_on_public_listen"), "{codes:?}");
    }

    #[test]
    fn admin_none_auth_is_allowed_on_loopback() {
        for listen in ["127.0.0.1:9091", "[::1]:9091"] {
            let mut config = valid_config();
            config.admin.listen = listen.into();
            assert!(
                !codes(&config.check()).contains(&"admin_open_on_public_listen"),
                "{listen}"
            );
        }
    }

    #[test]
    fn admin_cors_rejects_wildcard_and_bare_hosts() {
        let mut config = valid_config();
        config.admin.auth = AdminAuth::Token;
        config.admin.allow_origins = vec![
            "*".into(),
            "ops.example".into(),
            "https://ok.example".into(),
        ];
        let codes = codes(&config.check());
        assert_eq!(
            codes
                .iter()
                .filter(|c| **c == "admin_origin_not_explicit")
                .count(),
            2,
            "{codes:?}"
        );
    }
}

#[cfg(test)]
mod waf_config_tests {
    use super::*;

    const RULE: &str = "SecRule ARGS:attack \"@streq yes\" \"id:1,phase:1,deny\"\n";

    fn config_with_rules_file(file: &str) -> String {
        format!(
            r#"
[waf]
rules_file = "{file}"

[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:3000"

[[route]]
name = "app"
service = "app"
is_default = true
"#
        )
    }

    #[test]
    fn a_missing_rules_file_is_a_validation_error() {
        let text = config_with_rules_file("/definitely/not/here/rules.conf");
        let config = PrxConfig::from_toml_str(&text);
        // `from_toml_str` has no config directory, so the absolute path is what
        // gets checked here.
        assert!(config.is_err(), "missing rules file must fail validation");

        let parsed: PrxConfig = toml::from_str(&text).expect("parses");
        let issues = parsed.check();
        let issue = issues
            .iter()
            .find(|issue| issue.path == "waf.rules_file")
            .expect("an issue on waf.rules_file");
        assert_eq!(issue.severity, IssueSeverity::Error);
        assert_eq!(issue.code, "unreadable_file");
    }

    #[test]
    fn a_relative_rules_file_resolves_against_the_config_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("rules.conf"), RULE).expect("write rules");
        let config_path = dir.path().join("Prx.toml");
        std::fs::write(&config_path, config_with_rules_file("rules.conf")).expect("write config");

        let config =
            PrxConfig::from_file(&config_path).expect("relative file found next to config");
        assert_eq!(
            config.waf.resolved_rules_file(),
            Some(dir.path().join("rules.conf"))
        );
    }

    #[test]
    fn a_relative_rules_file_missing_next_to_the_config_is_rejected_at_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("Prx.toml");
        std::fs::write(&config_path, config_with_rules_file("nope.conf")).expect("write config");

        assert!(PrxConfig::from_file(&config_path).is_err());
    }

    #[test]
    fn a_config_without_waf_has_no_rules_file() {
        let text = r#"
[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:3000"

[[route]]
name = "app"
service = "app"
is_default = true
"#;
        let config = PrxConfig::from_toml_str(text).expect("valid");
        assert!(config.waf.resolved_rules_file().is_none());
        // Serialized form stays unchanged for configs that never set [waf].
        let rendered = toml::to_string(&config).expect("serializes");
        assert!(!rendered.contains("waf"), "{rendered}");
    }

    #[test]
    fn a_rules_file_becomes_the_runtime_ruleset() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("rules.conf"),
            format!(
                "{RULE}SecRule ARGS:b \"@streq x\" \"id:2,phase:1,deny\"\nSecRule ARGS:c \"@streq y\" \"id:3,phase:1,deny\"\n"
            ),
        )
        .expect("write rules");
        let config_path = dir.path().join("Prx.toml");
        std::fs::write(&config_path, config_with_rules_file("rules.conf")).expect("write config");

        let runtime = crate::runtime::RuntimeConfig::from_config(
            PrxConfig::from_file(&config_path).expect("loads"),
        );
        let rules: usize = runtime.ruleset.phases.iter().map(Vec::len).sum();
        assert_eq!(rules, 3, "all three rules reach the runtime");
    }

    #[test]
    fn no_waf_section_gives_the_empty_default_ruleset() {
        let text = config_with_rules_file("x.conf").replace("[waf]\nrules_file = \"x.conf\"\n", "");
        let runtime = crate::runtime::RuntimeConfig::from_config(
            PrxConfig::from_toml_str(&text).expect("valid"),
        );
        let rules: usize = runtime.ruleset.phases.iter().map(Vec::len).sum();
        assert_eq!(rules, 0);
    }
}

#[cfg(test)]
mod waf_tuning_tests {
    use super::*;

    fn with_waf_keys(keys: &str) -> PrxConfig {
        let text = format!(
            "[waf]\n{keys}\n\n[[service]]\nname = \"app\"\n\n[[service.upstream]]\naddr = \"127.0.0.1:3000\"\n\n[[route]]\nname = \"app\"\nservice = \"app\"\nis_default = true\n"
        );
        toml::from_str(&text).expect("parses")
    }

    fn waf_errors(config: &PrxConfig) -> Vec<(String, &'static str)> {
        config
            .check()
            .into_iter()
            .filter(|issue| {
                issue.path.starts_with("waf.") && issue.severity == IssueSeverity::Error
            })
            .map(|issue| (issue.path, issue.code))
            .collect()
    }

    #[test]
    fn tuning_defaults_to_paranoia_1_and_threshold_5() {
        let config = with_waf_keys("");
        assert_eq!(config.waf.paranoia_level(), 1);
        assert_eq!(config.waf.inbound_anomaly_threshold(), 5);
        assert!(waf_errors(&config).is_empty());
    }

    #[test]
    fn valid_tuning_is_accepted_and_used() {
        for level in 1..=4 {
            let keys = format!("paranoia_level = {level}\ninbound_anomaly_threshold = 7");
            let config = with_waf_keys(&keys);
            assert!(waf_errors(&config).is_empty(), "level {level}");
            assert_eq!(config.waf.paranoia_level(), level as u8);
            assert_eq!(config.waf.inbound_anomaly_threshold(), 7);
        }
    }

    #[test]
    fn out_of_range_paranoia_level_is_a_validation_error() {
        for level in [0, 5] {
            let config = with_waf_keys(&format!("paranoia_level = {level}"));
            assert_eq!(
                waf_errors(&config),
                [("waf.paranoia_level".to_string(), "paranoia_level_range")],
                "level {level}"
            );
            assert!(
                PrxConfig::from_toml_str(&format!("[waf]\nparanoia_level = {level}\n")).is_err()
            );
        }
    }

    #[test]
    fn zero_anomaly_threshold_is_a_validation_error() {
        let config = with_waf_keys("inbound_anomaly_threshold = 0");
        assert_eq!(
            waf_errors(&config),
            [(
                "waf.inbound_anomaly_threshold".to_string(),
                "inbound_anomaly_threshold_range"
            )]
        );
        assert!(PrxConfig::from_toml_str(
            "[waf]\ninbound_anomaly_threshold = 0\n\n[[service]]\nname = \"app\"\n\n[[service.upstream]]\naddr = \"127.0.0.1:3000\"\n\n[[route]]\nname = \"app\"\nservice = \"app\"\nis_default = true\n"
        )
        .is_err());
    }
}
