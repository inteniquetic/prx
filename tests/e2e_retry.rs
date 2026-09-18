//! T107: request time budget and retry safety, through the real binary.

mod common;

use std::time::{Duration, Instant};

use tempfile::TempDir;

use common::{
    CountingRefusedUpstream, PrxProcess, SlowUpstream, reserve_port, send_get, send_post,
    write_config,
};

/// A request that outlives `request_timeout_ms` must end as 504 rather than
/// hanging on for as long as the upstream feels like taking.
#[test]
fn a_request_over_its_time_budget_gets_504() {
    let upstream_port = reserve_port();
    let _upstream = SlowUpstream::spawn(upstream_port, Duration::from_secs(5));
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "slow"
max_retries = 2
request_timeout_ms = 400

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"
connect_timeout_ms = 1000
read_timeout_ms = 5000

[[route]]
name = "slow"
service = "slow"
path_prefix = "/"
is_default = true
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let started = Instant::now();
    let response = send_get(proxy_port, "slow.local", "/");
    let elapsed = started.elapsed();

    assert!(
        response.starts_with("HTTP/1.1 504"),
        "expected a 504 once the budget ran out, got:\n{response}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "the budget did not cut the request short: took {elapsed:?}"
    );
}

/// Retrying a POST that may already have been applied can double-charge a
/// customer. Only connect failures, where nothing was sent, may replay it.
#[test]
fn a_post_is_not_replayed_after_the_upstream_dropped_it() {
    let upstream_port = reserve_port();
    let upstream = CountingRefusedUpstream::spawn(upstream_port);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
max_retries = 3
retry_idempotent_only = true
retry_budget_ratio = 0.0

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let _ = send_post(proxy_port, "app.local", "/charge", "amount=100");
    let post_attempts = upstream.attempts();
    assert_eq!(
        post_attempts, 1,
        "a POST reached the upstream {post_attempts} times; it must be attempted once"
    );

    // A GET is safe to replay, so it does use the retry allowance.
    let before = upstream.attempts();
    let _ = send_get(proxy_port, "app.local", "/read");
    let get_attempts = upstream.attempts() - before;
    assert!(
        get_attempts > 1,
        "a GET should have been retried, saw {get_attempts} attempt(s)"
    );
}

/// With the budget in place, a dead upstream must not receive
/// `1 + max_retries` times the traffic at the worst possible moment.
#[test]
fn the_retry_budget_caps_amplification_against_a_dead_upstream() {
    let upstream_port = reserve_port();
    let upstream = CountingRefusedUpstream::spawn(upstream_port);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
max_retries = 3
retry_budget_ratio = 0.1
retry_budget_min_per_window = 3
retry_budget_window_ms = 60000

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let requests = 20;
    for _ in 0..requests {
        let _ = send_get(proxy_port, "app.local", "/");
    }

    let attempts = upstream.attempts();
    let without_budget = requests * 2; // one attempt plus retries per request
    assert!(
        attempts < without_budget,
        "the budget did not limit anything: {attempts} attempts for {requests} requests"
    );
    assert!(
        attempts >= requests,
        "every request should still have been attempted once, saw {attempts}"
    );
}
