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
