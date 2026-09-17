mod common;

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use tempfile::TempDir;

use common::{PrxProcess, UpstreamServer, reserve_port, send_get, write_config};

fn send_request(port: u16, method: &str, host: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to prx");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("failed to set read timeout");
    let req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .expect("failed to write request");
    stream.flush().expect("failed to flush request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("failed to read response");
    response
}

#[test]
fn routes_request_to_upstream() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "hello from upstream");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
lb = "round_robin"
max_retries = 0

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
host = "app.local"
path_prefix = "/"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "app.local", "/");

    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
    assert!(
        response.contains("hello from upstream"),
        "response: {response}"
    );
}

#[test]
fn returns_404_when_no_route_matches() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "unused");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "only"
lb = "round_robin"
max_retries = 0

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "only"
service = "only"
host = "only.local"
path_prefix = "/"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "other.local", "/");

    assert!(response.starts_with("HTTP/1.1 404"), "response: {response}");
}

#[test]
fn retries_and_fails_over_to_next_upstream() {
    let unreachable_port = reserve_port();
    let healthy_port = reserve_port();
    let _healthy = UpstreamServer::spawn(healthy_port, "served by failover");
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
lb = "round_robin"
max_retries = 1
retry_backoff_ms = 0

[[service.upstream]]
addr = "127.0.0.1:{unreachable_port}"

[[service.upstream]]
addr = "127.0.0.1:{healthy_port}"

[[route]]
name = "app"
service = "app"
host = "app.local"
path_prefix = "/"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "app.local", "/");

    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
    assert!(
        response.contains("served by failover"),
        "response: {response}"
    );
}

#[test]
fn serves_health_and_ready_endpoints() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "ok");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
lb = "round_robin"
max_retries = 0

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
host = "app.local"
path_prefix = "/"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);

    let health = send_get(proxy_port, "any.local", "/healthz");
    assert!(health.starts_with("HTTP/1.1 200"), "health: {health}");
    assert!(health.contains("ok"), "health: {health}");

    let ready = send_get(proxy_port, "any.local", "/readyz");
    assert!(ready.starts_with("HTTP/1.1 200"), "ready: {ready}");
    assert!(ready.contains("ready"), "ready: {ready}");
}

/// T102: the route index must pick the longest path prefix and the most
/// specific host, through the real proxy rather than only in unit tests.
#[test]
fn matches_the_most_specific_route() {
    let general_port = reserve_port();
    let _general = UpstreamServer::spawn(general_port, "general upstream");
    let specific_port = reserve_port();
    let _specific = UpstreamServer::spawn(specific_port, "specific upstream");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "general"

[[service.upstream]]
addr = "127.0.0.1:{general_port}"

[[service]]
name = "specific"

[[service.upstream]]
addr = "127.0.0.1:{specific_port}"

[[route]]
name = "wildcard"
service = "general"
host = "*.app.local"
path_prefix = "/"

[[route]]
name = "exact-host-deep-path"
service = "specific"
host = "api.app.local"
path_prefix = "/v1/users"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);

    // Exact host + longest prefix wins.
    let response = send_get(proxy_port, "api.app.local", "/v1/users/42");
    assert!(
        response.contains("specific upstream"),
        "response: {response}"
    );

    // Same host, shorter path: only the wildcard route covers it.
    let response = send_get(proxy_port, "api.app.local", "/v1/orders");
    assert!(
        response.contains("general upstream"),
        "response: {response}"
    );

    // Host with a port and different casing still normalizes to a match.
    let response = send_get(proxy_port, "API.App.Local:8080", "/v1/users/42");
    assert!(
        response.contains("specific upstream"),
        "response: {response}"
    );
}

/// T102: a route that lists methods rejects everything else with 405, and does
/// not silently fall through to a broader route.
#[test]
fn rejects_methods_a_route_does_not_allow() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "hello from upstream");
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
name = "catch-all"
service = "app"
host = "app.local"
path_prefix = "/"

[[route]]
name = "writes-only"
service = "app"
host = "app.local"
path_prefix = "/admin"
methods = ["POST"]
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let admin_port = reserve_port();

    let prx = PrxProcess::spawn(&cfg_path, admin_port);
    prx.wait_until_listening(proxy_port);

    let response = send_request(proxy_port, "POST", "app.local", "/admin/reset");
    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");

    let response = send_request(proxy_port, "GET", "app.local", "/admin/reset");
    assert!(response.starts_with("HTTP/1.1 405"), "response: {response}");

    // Paths outside the restricted prefix are unaffected.
    let response = send_request(proxy_port, "GET", "app.local", "/public");
    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
}
