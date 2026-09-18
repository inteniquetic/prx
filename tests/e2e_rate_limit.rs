//! T108: rate limiting and concurrency limiting, through the real binary.

mod common;

use std::{thread, time::Duration};

use tempfile::TempDir;

use common::{
    PrxProcess, SlowUpstream, UpstreamServer, reserve_port, send_get_with_headers, write_config,
};

fn status_of(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn start(limit_block: &str) -> (PrxProcess, u16, TempDir, UpstreamServer) {
    let upstream_port = reserve_port();
    let upstream = UpstreamServer::spawn(upstream_port, "hello");
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

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
{limit_block}
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);
    (prx, proxy_port, tmp, upstream)
}

#[test]
fn requests_over_the_limit_get_429_with_retry_after() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.rate_limit]
enabled = true
key = "client_ip"
requests_per_second = 2
burst = 3
"#,
    );

    // The burst is allowed.
    for i in 0..3 {
        let response = send_get_with_headers(port, "app.local", "/", &[]);
        assert_eq!(status_of(&response), 200, "burst request {i} was rejected");
    }

    // The next one is over the limit.
    let rejected = send_get_with_headers(port, "app.local", "/", &[]);
    assert_eq!(status_of(&rejected), 429, "expected 429, got:\n{rejected}");
    assert!(
        rejected.to_ascii_lowercase().contains("retry-after:"),
        "a rejection should say when to come back:\n{rejected}"
    );

    // After a refill window the client is served again.
    thread::sleep(Duration::from_millis(700));
    let after_wait = send_get_with_headers(port, "app.local", "/", &[]);
    assert_eq!(
        status_of(&after_wait),
        200,
        "the limit never refilled:\n{after_wait}"
    );
}

#[test]
fn traffic_under_the_limit_is_never_rejected() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.rate_limit]
enabled = true
key = "client_ip"
requests_per_second = 1000
burst = 1000
"#,
    );

    for i in 0..50 {
        let response = send_get_with_headers(port, "app.local", "/", &[]);
        assert_eq!(
            status_of(&response),
            200,
            "request {i} was rejected while well under the limit"
        );
    }
}

#[test]
fn each_header_value_gets_its_own_allowance() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.rate_limit]
enabled = true
key = "header:X-Api-Key"
requests_per_second = 1
burst = 1
"#,
    );

    // First key spends its allowance.
    assert_eq!(
        status_of(&send_get_with_headers(
            port,
            "app.local",
            "/",
            &["X-Api-Key: tenant-a"]
        )),
        200
    );
    assert_eq!(
        status_of(&send_get_with_headers(
            port,
            "app.local",
            "/",
            &["X-Api-Key: tenant-a"]
        )),
        429,
        "the same key should be limited"
    );

    // A different key is unaffected by the first one's usage.
    assert_eq!(
        status_of(&send_get_with_headers(
            port,
            "app.local",
            "/",
            &["X-Api-Key: tenant-b"]
        )),
        200,
        "another tenant must have its own allowance"
    );
}

#[test]
fn a_concurrency_limit_sheds_load_instead_of_queueing_it() {
    // A slow upstream keeps requests in flight long enough to collide.
    let upstream_port = reserve_port();
    let _upstream = SlowUpstream::spawn(upstream_port, Duration::from_millis(800));
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

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"
read_timeout_ms = 5000

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true

[route.concurrency_limit]
max_concurrent = 1
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    // One request occupies the only slot.
    let occupied = thread::spawn(move || send_get_with_headers(proxy_port, "app.local", "/", &[]));
    thread::sleep(Duration::from_millis(200));

    let rejected = send_get_with_headers(proxy_port, "app.local", "/", &[]);
    assert_eq!(
        status_of(&rejected),
        503,
        "a second concurrent request should be shed, got:\n{rejected}"
    );

    let first = occupied.join().expect("the first request panicked");
    assert_eq!(
        status_of(&first),
        200,
        "the in-flight request should finish"
    );

    // Once the slot is released the route works normally again.
    let after = send_get_with_headers(proxy_port, "app.local", "/", &[]);
    assert_eq!(
        status_of(&after),
        200,
        "the slot was never released:\n{after}"
    );
}
