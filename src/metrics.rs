use once_cell::sync::Lazy;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, register_histogram_vec,
    register_int_counter_vec, register_int_gauge_vec,
};

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
        ),
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
