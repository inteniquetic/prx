//! T106: per-route header rules, verified through the real binary.
//!
//! The upstream echoes the request head it received, so these tests assert on
//! exactly what prx forwarded rather than on internal state.

mod common;

use tempfile::TempDir;

use common::{EchoHeadersUpstream, PrxProcess, reserve_port, send_get_with_headers, write_config};

fn start(extra_config: &str) -> (PrxProcess, u16, TempDir, EchoHeadersUpstream) {
    let upstream_port = reserve_port();
    let upstream = EchoHeadersUpstream::spawn(upstream_port);
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
{extra_config}
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);
    (prx, proxy_port, tmp, upstream)
}

/// The echoed request head, lowercased for header comparisons.
fn upstream_saw(response: &str) -> String {
    response.to_ascii_lowercase()
}

#[test]
fn forwards_client_ip_to_the_upstream() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.request_headers]
set = { "X-Real-IP" = "$client_ip" }
add = { "X-Forwarded-For" = "$client_ip" }
"#,
    );

    let response = send_get_with_headers(port, "app.local", "/", &[]);
    let seen = upstream_saw(&response);

    assert!(
        seen.contains("x-real-ip: 127.0.0.1"),
        "upstream did not see the client ip:\n{response}"
    );
    assert!(
        seen.contains("x-forwarded-for: 127.0.0.1"),
        "upstream did not see x-forwarded-for:\n{response}"
    );
}

#[test]
fn add_appends_to_an_existing_forwarded_for_chain() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.request_headers]
add = { "X-Forwarded-For" = "$client_ip" }
"#,
    );

    let response = send_get_with_headers(port, "app.local", "/", &["X-Forwarded-For: 203.0.113.9"]);
    let seen = upstream_saw(&response);

    assert!(
        seen.contains("203.0.113.9"),
        "the existing chain was dropped:\n{response}"
    );
    assert!(
        seen.contains("127.0.0.1"),
        "prx did not append itself to the chain:\n{response}"
    );
}

#[test]
fn set_replaces_a_spoofed_header() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.request_headers]
set = { "X-Forwarded-For" = "$client_ip" }
"#,
    );

    let response = send_get_with_headers(port, "app.local", "/", &["X-Forwarded-For: 10.0.0.66"]);
    let seen = upstream_saw(&response);

    assert!(
        !seen.contains("10.0.0.66"),
        "a spoofed value survived a `set` rule:\n{response}"
    );
    assert!(seen.contains("x-forwarded-for: 127.0.0.1"), "{response}");
}

#[test]
fn removes_internal_headers_before_the_upstream_sees_them() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.request_headers]
remove = ["X-Internal-Token"]
"#,
    );

    let response =
        send_get_with_headers(port, "app.local", "/", &["X-Internal-Token: super-secret"]);

    assert!(
        !upstream_saw(&response).contains("super-secret"),
        "an internal header leaked to the upstream:\n{response}"
    );
}

#[test]
fn rewrites_response_headers() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.response_headers]
set = { "X-Frame-Options" = "DENY" }
remove = ["Server"]
"#,
    );

    let response = send_get_with_headers(port, "app.local", "/", &[]);
    let head = response
        .split("\r\n\r\n")
        .next()
        .expect("response has no head")
        .to_ascii_lowercase();

    assert!(head.contains("x-frame-options: deny"), "{response}");
    assert!(
        !head.contains("server: test-upstream"),
        "the upstream Server header was not removed:\n{response}"
    );
}

#[test]
fn generates_a_request_id_and_reuses_an_inbound_one() {
    let (_prx, port, _tmp, _upstream) = start(
        r#"
[route.request_headers]
set = { "X-Request-Id" = "$request_id" }
"#,
    );

    let generated = send_get_with_headers(port, "app.local", "/", &[]);
    let seen = upstream_saw(&generated);
    let id_line = seen
        .lines()
        .find(|line| line.starts_with("x-request-id:"))
        .unwrap_or_else(|| panic!("no request id was generated:\n{generated}"));
    assert!(
        id_line.trim_start_matches("x-request-id:").trim().len() >= 8,
        "request id looks empty: {id_line}"
    );

    let reused = send_get_with_headers(port, "app.local", "/", &["X-Request-Id: trace-me-123"]);
    assert!(
        upstream_saw(&reused).contains("trace-me-123"),
        "an inbound request id was not carried through:\n{reused}"
    );
}

#[test]
fn global_rules_apply_to_every_route() {
    let upstream_port = reserve_port();
    let _upstream = EchoHeadersUpstream::spawn(upstream_port);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[headers.request]
set = {{ "X-Edge" = "prx" }}

[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true

[route.request_headers]
set = {{ "X-Route" = "$route_name" }}
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let response = send_get_with_headers(proxy_port, "app.local", "/", &[]);
    let seen = upstream_saw(&response);

    assert!(
        seen.contains("x-edge: prx"),
        "global rule missing:\n{response}"
    );
    assert!(
        seen.contains("x-route: app"),
        "route rule missing:\n{response}"
    );
}
