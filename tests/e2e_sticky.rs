//! T114: session affinity through the real binary.

mod common;

use tempfile::TempDir;

use common::{PrxProcess, UpstreamServer, reserve_port, send_get_with_headers, write_config};

fn config(proxy_port: u16, a: u16, b: u16, mode: &str, extra: &str) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
lb = "round_robin"
max_retries = 1

[service.sticky]
enabled = true
mode = "{mode}"
{extra}

[[service.upstream]]
addr = "127.0.0.1:{a}"

[[service.upstream]]
addr = "127.0.0.1:{b}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    )
}

/// Extracts the sticky cookie prx handed out, as a "name=value" pair ready to
/// send back.
fn sticky_cookie(response: &str, name: &str) -> Option<String> {
    response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim())
        .and_then(|value| value.split(';').next())
        .filter(|pair| pair.starts_with(&format!("{name}=")))
        .map(|pair| pair.to_string())
}

#[test]
fn a_cookie_keeps_a_client_on_the_same_upstream() {
    let a_port = reserve_port();
    let b_port = reserve_port();
    let _a = UpstreamServer::spawn(a_port, "upstream A");
    let _b = UpstreamServer::spawn(b_port, "upstream B");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &config(
            proxy_port,
            a_port,
            b_port,
            "cookie",
            "name = \"prx_upstream\"\nttl_s = 600",
        ),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    // First request: no cookie yet, so prx picks one and says which.
    let first = send_get_with_headers(proxy_port, "app.local", "/", &[]);
    let cookie = sticky_cookie(&first, "prx_upstream")
        .unwrap_or_else(|| panic!("prx did not set an affinity cookie:\n{first}"));
    let pinned = if first.contains("upstream A") {
        "upstream A"
    } else {
        "upstream B"
    };

    // Every later request carrying the cookie must land on the same upstream,
    // even though the strategy is round robin.
    let cookie_header = format!("Cookie: {cookie}");
    for _ in 0..10 {
        let response = send_get_with_headers(proxy_port, "app.local", "/", &[&cookie_header]);
        assert!(
            response.contains(pinned),
            "affinity broke: expected {pinned}, got:\n{response}"
        );
    }

    // A client without the cookie is still load balanced normally.
    let mut seen_other = false;
    for _ in 0..6 {
        if !send_get_with_headers(proxy_port, "app.local", "/", &[]).contains(pinned) {
            seen_other = true;
        }
    }
    assert!(
        seen_other,
        "requests without a cookie should still reach both upstreams"
    );
}

#[test]
fn affinity_gives_way_when_the_pinned_upstream_disappears() {
    let a_port = reserve_port();
    let b_port = reserve_port();
    let a = UpstreamServer::spawn(a_port, "upstream A");
    let _b = UpstreamServer::spawn(b_port, "upstream B");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(
        &tmp,
        &config(
            proxy_port,
            a_port,
            b_port,
            "cookie",
            "name = \"prx_upstream\"",
        ),
    );
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    // Pin a client to upstream A by replaying until the cookie points there.
    let mut cookie_header = None;
    for _ in 0..10 {
        let response = send_get_with_headers(proxy_port, "app.local", "/", &[]);
        if response.contains("upstream A")
            && let Some(cookie) = sticky_cookie(&response, "prx_upstream")
        {
            cookie_header = Some(format!("Cookie: {cookie}"));
            break;
        }
    }
    let cookie_header = cookie_header.expect("never got pinned to upstream A");

    // Take A away. Affinity cannot know that before trying, so the retry is
    // what moves the client; the request must still succeed on B.
    drop(a);
    let response = send_get_with_headers(proxy_port, "app.local", "/", &[&cookie_header]);
    assert!(
        response.contains("upstream B"),
        "a pinned client should fall back to a healthy upstream, got:\n{response}"
    );
}

#[test]
fn client_ip_mode_pins_without_a_cookie() {
    let a_port = reserve_port();
    let b_port = reserve_port();
    let _a = UpstreamServer::spawn(a_port, "upstream A");
    let _b = UpstreamServer::spawn(b_port, "upstream B");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg_path = write_config(&tmp, &config(proxy_port, a_port, b_port, "client_ip", ""));
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    let first = send_get_with_headers(proxy_port, "app.local", "/", &[]);
    let pinned = if first.contains("upstream A") {
        "upstream A"
    } else {
        "upstream B"
    };

    // Every request comes from the same loopback address, so they must all
    // land on the same upstream without any cookie being involved.
    for _ in 0..10 {
        let response = send_get_with_headers(proxy_port, "app.local", "/", &[]);
        assert!(
            response.contains(pinned),
            "client_ip affinity broke: expected {pinned}, got:\n{response}"
        );
    }
}
