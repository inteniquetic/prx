//! T109: response cache and request coalescing, through the real binary.

mod common;

use std::{thread, time::Duration};

use tempfile::TempDir;

use common::{
    CountingUpstream, HeaderControlledUpstream, PrxProcess, reserve_port, send_get_with_headers,
    write_config,
};

fn cache_config(proxy_port: u16, upstream_port: u16, cache_block: &str) -> String {
    format!(
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

[route.cache]
enabled = true
{cache_block}
"#
    )
}

fn header_present(response: &str, needle: &str) -> bool {
    response
        .split("\r\n\r\n")
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains(needle)
}

#[test]
fn a_second_request_is_served_without_touching_the_upstream() {
    let upstream_port = reserve_port();
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::ZERO);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 5000"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let first = send_get_with_headers(proxy_port, "app.local", "/thing", &[]);
    assert!(first.contains("payload"), "unexpected response:\n{first}");
    assert!(header_present(&first, "x-cache: miss"), "{first}");
    assert_eq!(upstream.hits(), 1);

    for _ in 0..10 {
        let response = send_get_with_headers(proxy_port, "app.local", "/thing", &[]);
        assert!(response.contains("payload"), "{response}");
        assert!(header_present(&response, "x-cache: hit"), "{response}");
    }
    assert_eq!(
        upstream.hits(),
        1,
        "the upstream should have been asked exactly once"
    );

    // A different path is a different key.
    let other = send_get_with_headers(proxy_port, "app.local", "/other", &[]);
    assert!(header_present(&other, "x-cache: miss"), "{other}");
    assert_eq!(upstream.hits(), 2);
}

/// The reason the cache exists: a burst on a cold key must not multiply into a
/// burst on the upstream.
#[test]
fn concurrent_misses_reach_the_upstream_once() {
    let upstream_port = reserve_port();
    // Slow enough that every request piles up on the same cold key.
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::from_millis(300));
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(
            proxy_port,
            upstream_port,
            "ttl_ms = 5000\ncoalesce_wait_ms = 3000",
        ),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let mut clients = Vec::new();
    for _ in 0..30 {
        clients.push(thread::spawn(move || {
            send_get_with_headers(proxy_port, "app.local", "/hot", &[])
        }));
    }

    let mut served = 0;
    for client in clients {
        let response = client.join().expect("client panicked");
        if response.contains("payload") {
            served += 1;
        }
    }

    assert_eq!(served, 30, "every client should have been served");
    assert_eq!(
        upstream.hits(),
        1,
        "30 concurrent requests produced {} upstream fetches; coalescing did not work",
        upstream.hits()
    );
}

#[test]
fn entries_expire_and_are_fetched_again() {
    let upstream_port = reserve_port();
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::ZERO);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 300"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    send_get_with_headers(proxy_port, "app.local", "/thing", &[]);
    send_get_with_headers(proxy_port, "app.local", "/thing", &[]);
    assert_eq!(upstream.hits(), 1);

    thread::sleep(Duration::from_millis(500));
    let after = send_get_with_headers(proxy_port, "app.local", "/thing", &[]);
    assert!(header_present(&after, "x-cache: miss"), "{after}");
    assert_eq!(upstream.hits(), 2, "an expired entry must be refetched");
}

#[test]
fn a_private_response_is_never_shared() {
    let upstream_port = reserve_port();
    let upstream = HeaderControlledUpstream::spawn(
        upstream_port,
        "cache-control: private, max-age=60\r\n",
        "secret",
    );
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 5000"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    for _ in 0..3 {
        send_get_with_headers(proxy_port, "app.local", "/me", &[]);
    }
    assert_eq!(
        upstream.hits(),
        3,
        "a private response must be fetched for every client"
    );
}

#[test]
fn a_response_that_sets_a_cookie_is_never_shared() {
    let upstream_port = reserve_port();
    let upstream =
        HeaderControlledUpstream::spawn(upstream_port, "set-cookie: session=abc\r\n", "hello");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 5000"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    for _ in 0..3 {
        send_get_with_headers(proxy_port, "app.local", "/login", &[]);
    }
    assert_eq!(
        upstream.hits(),
        3,
        "caching a Set-Cookie response would hand one client's session to another"
    );
}

#[test]
fn an_authenticated_request_bypasses_the_cache() {
    let upstream_port = reserve_port();
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::ZERO);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 5000"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    for _ in 0..3 {
        send_get_with_headers(
            proxy_port,
            "app.local",
            "/account",
            &["Authorization: Bearer token123"],
        );
    }
    assert_eq!(
        upstream.hits(),
        3,
        "an authenticated response must not be served to anyone else"
    );
}

#[test]
fn a_body_over_the_limit_is_streamed_but_not_stored() {
    let upstream_port = reserve_port();
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::ZERO);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    // Smaller than the response the upstream produces.
    let cfg_path = write_config(
        &tmp,
        &cache_config(
            proxy_port,
            upstream_port,
            "ttl_ms = 5000\nmax_body_bytes = 4",
        ),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    for _ in 0..3 {
        let response = send_get_with_headers(proxy_port, "app.local", "/big", &[]);
        assert!(
            response.contains("payload"),
            "the response must still be delivered:\n{response}"
        );
    }
    assert_eq!(
        upstream.hits(),
        3,
        "an oversized body must not be stored, only streamed"
    );
}

#[test]
fn a_post_is_never_served_from_the_cache() {
    let upstream_port = reserve_port();
    let upstream = CountingUpstream::spawn(upstream_port, "payload", Duration::ZERO);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &cache_config(proxy_port, upstream_port, "ttl_ms = 5000"),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    // Prime the cache with a GET on the same path.
    send_get_with_headers(proxy_port, "app.local", "/submit", &[]);
    assert_eq!(upstream.hits(), 1);

    for _ in 0..2 {
        common::send_post(proxy_port, "app.local", "/submit", "data=1");
    }
    assert_eq!(upstream.hits(), 3, "a POST must always reach the upstream");
}
