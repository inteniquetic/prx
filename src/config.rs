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
    #[serde(default)]
    pub compression: CompressionConfig,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warning,
}

/// One problem found in a config, addressed by field path so the editor can
/// turn it into a mark on the right line (T203).
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
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

/// Response compression.
///
/// prx uses pingora's streaming compressor, so a large response is compressed
/// as it flows through rather than being buffered first.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
}

/// Short-lived response cache for a route.
///
/// Even a one-second TTL turns a burst of identical requests into a single
/// upstream fetch, which is where most of the benefit comes from.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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
            observability: ObservabilityConfig::default(),
            headers: Default::default(),
            compression: Default::default(),
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
