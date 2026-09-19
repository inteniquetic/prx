//! T501: the plugin chain, through the real binary.
//!
//! The unit tests prove the chain is built right; these prove it is reached.
//! The upstream echoes the request head it was given, so what a plugin did on
//! the way in is visible in the response rather than in internal state.

mod common;

use tempfile::TempDir;

use common::{EchoHeadersUpstream, PrxProcess, reserve_port, send_get, write_config};

fn start(plugins: &str, route_plugins: &str) -> (PrxProcess, u16, TempDir, EchoHeadersUpstream) {
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
{plugins}
[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
{route_plugins}
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);
    (prx, proxy_port, tmp, upstream)
}

fn echo_plugin(name: &str, value: &str) -> String {
    format!(
        r#"
[[plugin]]
name = "{name}"
kind = "echo-header"

[plugin.config]
request_header = "x-mark"
value = "{value}"
"#
    )
}

#[test]
fn a_plugin_reaches_the_upstream_request() {
    let (_prx, port, _tmp, _upstream) = start(&echo_plugin("one", "alpha"), r#"plugins = ["one"]"#);

    let response = send_get(port, "app.local", "/").to_ascii_lowercase();
    assert!(
        response.contains("x-mark:"),
        "the plugin never ran: the upstream saw no mark\n{response}"
    );
    assert!(
        response.contains("alpha"),
        "the plugin ran but its value did not arrive\n{response}"
    );
}

#[test]
fn plugins_run_in_the_order_the_route_lists_them() {
    let config = format!(
        "{}{}",
        echo_plugin("one", "alpha"),
        echo_plugin("two", "beta")
    );
    let (_prx, port, _tmp, _upstream) = start(&config, r#"plugins = ["two", "one"]"#);

    let response = send_get(port, "app.local", "/").to_ascii_lowercase();
    let mark = response
        .lines()
        .find(|line| line.starts_with("x-mark:"))
        .unwrap_or_else(|| panic!("no mark header in:\n{response}"))
        .to_string();

    // Each plugin appends, so the header records the order they ran in — and
    // the route listed "two" first, not the [[plugin]] blocks' order.
    let beta = mark.find("beta").expect("second plugin did not run");
    let alpha = mark.find("alpha").expect("first plugin did not run");
    assert!(
        beta < alpha,
        "plugins ran in the order they were declared, not the order the route listed: {mark}"
    );
}

#[test]
fn a_plugin_can_answer_the_request_itself() {
    let plugin = r#"
[[plugin]]
name = "gate"
kind = "echo-header"

[plugin.config]
value = "blocked"
respond_status = 403
respond_body = "denied by plugin\n"
"#;
    let (_prx, port, _tmp, _upstream) = start(plugin, r#"plugins = ["gate"]"#);

    let response = send_get(port, "app.local", "/");
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "the plugin did not answer the request:\n{response}"
    );
    assert!(
        response.contains("denied by plugin"),
        "the plugin's body did not arrive:\n{response}"
    );
    // The upstream echoes what it received; if the chain had not stopped, its
    // echo would be in the body instead.
    assert!(
        !response.to_ascii_lowercase().contains("x-mark"),
        "the request reached the upstream despite being answered:\n{response}"
    );
}

#[test]
fn a_plugin_can_add_a_header_on_the_way_back() {
    let plugin = r#"
[[plugin]]
name = "stamp"
kind = "echo-header"

[plugin.config]
response_header = "x-prx-plugin"
value = "stamped"
"#;
    let (_prx, port, _tmp, _upstream) = start(plugin, r#"plugins = ["stamp"]"#);

    let response = send_get(port, "app.local", "/").to_ascii_lowercase();
    assert!(
        response.contains("x-prx-plugin: stamped"),
        "the response phase never ran:\n{response}"
    );
}

#[test]
fn a_route_without_plugins_is_untouched() {
    // The plugin exists and is enabled, but this route does not list it.
    let (_prx, port, _tmp, _upstream) = start(&echo_plugin("one", "alpha"), "");

    let response = send_get(port, "app.local", "/").to_ascii_lowercase();
    assert!(
        !response.contains("x-mark"),
        "a plugin ran on a route that never asked for it:\n{response}"
    );
}
