use std::collections::HashMap;

use once_cell::sync::Lazy;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, core::Collector, proto,
    register_histogram_vec, register_int_counter_vec, register_int_gauge, register_int_gauge_vec,
};

/// Latency buckets, in milliseconds, from "faster than we can measure" to
/// "the client gave up".
///
/// The default Prometheus buckets stop at 10 — which, for a metric counted in
/// milliseconds, put every request slower than 10 ms into `+Inf` and made every
/// percentile above p50 a guess.
const LATENCY_BUCKETS_MS: &[f64] = &[
    1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
];

static REQUESTS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_requests_total",
        "Total requests processed by prx",
        &["route", "status"]
    )
    .expect("failed to register prx_requests_total")
});

static REQUEST_LATENCY_MS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        HistogramOpts::new(
            "prx_request_latency_ms",
            "Request latency in milliseconds for prx"
        )
        .buckets(LATENCY_BUCKETS_MS.to_vec()),
        &["route"]
    )
    .expect("failed to register prx_request_latency_ms")
});

static UPSTREAM_ERRORS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_upstream_errors_total",
        "Upstream errors grouped by route/upstream/stage",
        &["route", "upstream", "stage"]
    )
    .expect("failed to register prx_upstream_errors_total")
});

static CIRCUIT_OPEN_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_circuit_breaker_open_total",
        "Number of times an upstream circuit opened",
        &["route", "upstream"]
    )
    .expect("failed to register prx_circuit_breaker_open_total")
});

static CIRCUIT_OPEN_STATE: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_upstream_circuit_open",
        "Current circuit breaker state (1=open, 0=closed)",
        &["route", "upstream"]
    )
    .expect("failed to register prx_upstream_circuit_open")
});

static RETRY_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_retry_total",
        "Retries attempted, grouped by route and reason",
        &["route", "reason"]
    )
    .expect("failed to register prx_retry_total")
});

static RETRY_DENIED_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_retry_denied_total",
        "Retries refused, grouped by route and why",
        &["route", "reason"]
    )
    .expect("failed to register prx_retry_denied_total")
});

static REQUEST_TIMEOUT_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_request_timeout_total",
        "Requests that ran out of their total time budget",
        &["route"]
    )
    .expect("failed to register prx_request_timeout_total")
});

static HEALTH_CHECK_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_health_check_total",
        "Active health check probes, grouped by service/upstream/result",
        &["service", "upstream", "result"]
    )
    .expect("failed to register prx_health_check_total")
});

static UPSTREAM_HEALTHY: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_upstream_healthy",
        "Whether the active health check considers an upstream usable (1=yes)",
        &["service", "upstream"]
    )
    .expect("failed to register prx_upstream_healthy")
});

static UPSTREAM_INFLIGHT: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_upstream_inflight",
        "Requests currently in flight per upstream",
        &["service", "upstream"]
    )
    .expect("failed to register prx_upstream_inflight")
});

static UPSTREAM_EWMA_MS: Lazy<prometheus::GaugeVec> = Lazy::new(|| {
    prometheus::register_gauge_vec!(
        "prx_upstream_ewma_ms",
        "Moving average latency per upstream, in milliseconds",
        &["service", "upstream"]
    )
    .expect("failed to register prx_upstream_ewma_ms")
});

pub fn set_upstream_inflight(service: &str, upstream: &str, inflight: usize) {
    UPSTREAM_INFLIGHT
        .with_label_values(&[service, upstream])
        .set(inflight as i64);
}

pub fn set_upstream_ewma_ms(service: &str, upstream: &str, ewma_ms: f64) {
    UPSTREAM_EWMA_MS
        .with_label_values(&[service, upstream])
        .set(ewma_ms);
}

pub fn inc_health_check(service: &str, upstream: &str, result: &str) {
    HEALTH_CHECK_TOTAL
        .with_label_values(&[service, upstream, result])
        .inc();
}

pub fn set_upstream_healthy(service: &str, upstream: &str, healthy: bool) {
    UPSTREAM_HEALTHY
        .with_label_values(&[service, upstream])
        .set(if healthy { 1 } else { 0 });
}

static RATE_LIMITED_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_rate_limited_total",
        "Requests rejected by a limit, grouped by route and which limit",
        &["route", "kind"]
    )
    .expect("failed to register prx_rate_limited_total")
});

static LIMITER_ENTRIES: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_limiter_entries",
        "Keys currently tracked by a route's rate limiter",
        &["route"]
    )
    .expect("failed to register prx_limiter_entries")
});

static CACHE_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "prx_cache_total",
        "Cache lookups by outcome (hit, miss, miss_follower)",
        &["route", "result"]
    )
    .expect("failed to register prx_cache_total")
});

static CACHE_ENTRIES: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_cache_entries",
        "Responses currently stored per route",
        &["route"]
    )
    .expect("failed to register prx_cache_entries")
});

static CACHE_BYTES: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_cache_bytes",
        "Bytes currently stored per route",
        &["route"]
    )
    .expect("failed to register prx_cache_bytes")
});

static TLS_CERT_EXPIRY: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "prx_tls_cert_expiry_seconds",
        "Seconds until a TLS certificate expires, per domain",
        &["domain"]
    )
    .expect("failed to register prx_tls_cert_expiry_seconds")
});

pub fn set_tls_cert_expiry(domain: &str, seconds_remaining: i64) {
    TLS_CERT_EXPIRY
        .with_label_values(&[domain])
        .set(seconds_remaining);
}

pub fn inc_cache(route: &str, result: &str) {
    CACHE_TOTAL.with_label_values(&[route, result]).inc();
}

pub fn set_cache_size(route: &str, entries: usize, bytes: usize) {
    CACHE_ENTRIES
        .with_label_values(&[route])
        .set(entries as i64);
    CACHE_BYTES.with_label_values(&[route]).set(bytes as i64);
}

pub fn inc_rate_limited(route: &str, kind: &str) {
    RATE_LIMITED_TOTAL.with_label_values(&[route, kind]).inc();
}

pub fn set_limiter_entries(route: &str, entries: usize) {
    LIMITER_ENTRIES
        .with_label_values(&[route])
        .set(entries as i64);
}

pub fn inc_retry(route: &str, reason: &str) {
    RETRY_TOTAL.with_label_values(&[route, reason]).inc();
}

pub fn inc_retry_denied(route: &str, reason: &str) {
    RETRY_DENIED_TOTAL.with_label_values(&[route, reason]).inc();
}

pub fn inc_request_timeout(route: &str) {
    REQUEST_TIMEOUT_TOTAL.with_label_values(&[route]).inc();
}

pub fn observe_request(route: &str, status: u16, latency_ms: f64) {
    let status_label = status.to_string();
    REQUESTS_TOTAL
        .with_label_values(&[route, status_label.as_str()])
        .inc();
    REQUEST_LATENCY_MS
        .with_label_values(&[route])
        .observe(latency_ms);
}

pub fn inc_upstream_error(route: &str, upstream: &str, stage: &str) {
    UPSTREAM_ERRORS_TOTAL
        .with_label_values(&[route, upstream, stage])
        .inc();
}

pub fn mark_circuit_open(route: &str, upstream: &str) {
    CIRCUIT_OPEN_TOTAL
        .with_label_values(&[route, upstream])
        .inc();
    CIRCUIT_OPEN_STATE
        .with_label_values(&[route, upstream])
        .set(1);
}

pub fn set_circuit_state(route: &str, upstream: &str, is_open: bool) {
    CIRCUIT_OPEN_STATE
        .with_label_values(&[route, upstream])
        .set(if is_open { 1 } else { 0 });
}

static INFLIGHT_REQUESTS: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!(
        "prx_inflight_requests",
        "Requests currently being served, counted across every listener"
    )
    .expect("failed to register prx_inflight_requests")
});

/// One relaxed atomic per request each way. Anything heavier than this does not
/// belong on the request path.
pub fn inc_inflight() {
    INFLIGHT_REQUESTS.inc();
}

pub fn dec_inflight() {
    INFLIGHT_REQUESTS.dec();
}

// ---------------------------------------------------------------------------
// Reading the registry back (T207)
// ---------------------------------------------------------------------------
//
// The live-stats sampler needs the same numbers `/metrics` serves, so it reads
// them from the registry once a second instead of counting a second time in the
// request path. Everything below runs on the sampler's task, never on a
// request.

/// Cumulative request counters for one route, as the registry holds them.
#[derive(Debug, Default, Clone)]
pub struct RouteCounters {
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    /// Requests whose status the proxy never got to write (aborted, upgraded).
    pub status_other: u64,
    /// Cumulative bucket counts, aligned with [`LATENCY_BUCKETS_MS`].
    pub latency_buckets: Vec<u64>,
    pub latency_count: u64,
    pub latency_sum_ms: f64,
}

impl RouteCounters {
    pub fn total(&self) -> u64 {
        self.status_2xx + self.status_3xx + self.status_4xx + self.status_5xx + self.status_other
    }
}

/// Everything the sampler reads in one pass over the registry.
#[derive(Debug, Default, Clone)]
pub struct MetricsSnapshot {
    pub routes: HashMap<String, RouteCounters>,
    pub cache_hits: u64,
    pub cache_lookups: u64,
    pub inflight: i64,
    /// `(service, upstream)` pairs whose circuit is currently open.
    pub circuits_open: Vec<(String, String)>,
    /// Seconds until expiry, per certificate domain.
    pub cert_expiry_seconds: Vec<(String, i64)>,
}

pub fn latency_bucket_bounds() -> &'static [f64] {
    LATENCY_BUCKETS_MS
}

fn label(metric: &proto::Metric, name: &str) -> String {
    metric
        .get_label()
        .iter()
        .find(|pair| pair.name() == name)
        .map(|pair| pair.value().to_string())
        .unwrap_or_default()
}

/// Reads the current value of every metric the dashboard needs.
///
/// Cost is proportional to the number of label values, not to traffic, and it
/// happens once a second on the admin runtime.
pub fn snapshot() -> MetricsSnapshot {
    let mut snapshot = MetricsSnapshot::default();

    for family in REQUESTS_TOTAL.collect() {
        for metric in family.get_metric() {
            let route = label(metric, "route");
            let status = label(metric, "status");
            let value = metric.get_counter().value() as u64;
            let entry = snapshot.routes.entry(route).or_default();
            match status.as_bytes().first() {
                Some(b'2') => entry.status_2xx += value,
                Some(b'3') => entry.status_3xx += value,
                Some(b'4') => entry.status_4xx += value,
                Some(b'5') => entry.status_5xx += value,
                _ => entry.status_other += value,
            }
        }
    }

    for family in REQUEST_LATENCY_MS.collect() {
        for metric in family.get_metric() {
            let route = label(metric, "route");
            let histogram = metric.get_histogram();
            let entry = snapshot.routes.entry(route).or_default();
            entry.latency_count = histogram.get_sample_count();
            entry.latency_sum_ms = histogram.get_sample_sum();
            entry.latency_buckets = histogram
                .get_bucket()
                .iter()
                .map(|bucket| bucket.cumulative_count())
                .collect();
        }
    }

    for family in CACHE_TOTAL.collect() {
        for metric in family.get_metric() {
            let value = metric.get_counter().value() as u64;
            snapshot.cache_lookups += value;
            if label(metric, "result") == "hit" {
                snapshot.cache_hits += value;
            }
        }
    }

    for family in CIRCUIT_OPEN_STATE.collect() {
        for metric in family.get_metric() {
            if metric.get_gauge().value() > 0.0 {
                snapshot
                    .circuits_open
                    .push((label(metric, "route"), label(metric, "upstream")));
            }
        }
    }

    for family in TLS_CERT_EXPIRY.collect() {
        for metric in family.get_metric() {
            snapshot
                .cert_expiry_seconds
                .push((label(metric, "domain"), metric.get_gauge().value() as i64));
        }
    }

    snapshot.inflight = INFLIGHT_REQUESTS.get();
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_groups_requests_by_status_class() {
        observe_request("snapshot-route", 204, 3.0);
        observe_request("snapshot-route", 404, 7.0);
        observe_request("snapshot-route", 503, 900.0);

        let snapshot = snapshot();
        let route = snapshot
            .routes
            .get("snapshot-route")
            .expect("the route should be in the snapshot");

        assert_eq!(route.status_2xx, 1);
        assert_eq!(route.status_4xx, 1);
        assert_eq!(route.status_5xx, 1);
        assert_eq!(route.latency_count, 3);
        assert_eq!(route.latency_buckets.len(), LATENCY_BUCKETS_MS.len());
    }

    #[test]
    fn latency_buckets_reach_past_ten_milliseconds() {
        // The default buckets stopped at 10, which made every percentile above
        // p50 useless for a proxy. Guard the fix.
        let bounds = latency_bucket_bounds();
        assert!(bounds.contains(&1000.0));
        assert_eq!(bounds.last().copied(), Some(10000.0));
    }

    #[test]
    fn inflight_gauge_counts_both_ways() {
        let before = snapshot().inflight;
        inc_inflight();
        inc_inflight();
        assert_eq!(snapshot().inflight, before + 2);
        dec_inflight();
        dec_inflight();
        assert_eq!(snapshot().inflight, before);
    }
}
