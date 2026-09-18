//! T110: response compression, through the real binary.

mod common;

use tempfile::TempDir;

use common::{
    BigBodyUpstream, PrxProcess, reserve_port, send_get_raw, split_response, write_config,
};

const BODY_LEN: usize = 64 * 1024;

fn config(proxy_port: u16, upstream_port: u16, compression: &str) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

{compression}

[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    )
}

fn start(
    compression: &str,
    upstream_headers: &'static str,
) -> (PrxProcess, u16, TempDir, BigBodyUpstream) {
    let upstream_port = reserve_port();
    let upstream = BigBodyUpstream::spawn(upstream_port, BODY_LEN, upstream_headers);
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg_path = write_config(&tmp, &config(proxy_port, upstream_port, compression));
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);
    (prx, proxy_port, tmp, upstream)
}

#[test]
fn a_compressible_body_is_compressed_when_the_client_accepts_it() {
    let (_prx, port, _tmp, _upstream) = start("[compression]\nenabled = true\nlevel = 4", "");

    let raw = send_get_raw(port, "app.local", "/big", &["Accept-Encoding: gzip"]);
    let (head, body) = split_response(&raw);
    let head = head.to_ascii_lowercase();

    assert!(
        head.contains("content-encoding: gzip"),
        "expected a gzip response:\n{head}"
    );
    assert!(
        body.len() < BODY_LEN / 4,
        "a body of {BODY_LEN} highly compressible bytes came back as {} bytes",
        body.len()
    );
}

#[test]
fn a_client_that_does_not_ask_gets_the_body_untouched() {
    let (_prx, port, _tmp, _upstream) = start("[compression]\nenabled = true\nlevel = 4", "");

    let raw = send_get_raw(port, "app.local", "/big", &[]);
    let (head, body) = split_response(&raw);

    assert!(
        !head.to_ascii_lowercase().contains("content-encoding:"),
        "a client that sent no Accept-Encoding must not get an encoded body:\n{head}"
    );
    assert_eq!(body.len(), BODY_LEN, "the body was altered");
}

#[test]
fn an_already_compressed_response_is_not_compressed_again() {
    // The upstream claims the body is already gzip encoded.
    let (_prx, port, _tmp, _upstream) = start(
        "[compression]\nenabled = true\nlevel = 4",
        "content-encoding: gzip\r\n",
    );

    let raw = send_get_raw(port, "app.local", "/big", &["Accept-Encoding: gzip"]);
    let (head, body) = split_response(&raw);
    let head = head.to_ascii_lowercase();

    assert_eq!(
        head.matches("content-encoding").count(),
        1,
        "the response should carry exactly one Content-Encoding:\n{head}"
    );
    assert_eq!(
        body.len(),
        BODY_LEN,
        "prx re-compressed a body the upstream had already encoded"
    );
}

#[test]
fn compression_stays_off_until_it_is_configured() {
    let (_prx, port, _tmp, _upstream) = start("", "");

    let raw = send_get_raw(port, "app.local", "/big", &["Accept-Encoding: gzip, br"]);
    let (head, body) = split_response(&raw);

    assert!(
        !head.to_ascii_lowercase().contains("content-encoding:"),
        "compression must be opt-in:\n{head}"
    );
    assert_eq!(body.len(), BODY_LEN);
}

#[test]
fn brotli_and_zstd_are_negotiated_too() {
    let (_prx, port, _tmp, _upstream) = start("[compression]\nenabled = true\nlevel = 4", "");

    for (accept, expected) in [("br", "br"), ("zstd", "zstd")] {
        let raw = send_get_raw(
            port,
            "app.local",
            "/big",
            &[&format!("Accept-Encoding: {accept}")],
        );
        let (head, body) = split_response(&raw);
        let head = head.to_ascii_lowercase();

        assert!(
            head.contains(&format!("content-encoding: {expected}")),
            "expected {expected} for Accept-Encoding: {accept}:\n{head}"
        );
        assert!(
            body.len() < BODY_LEN / 4,
            "{expected} produced {} bytes from {BODY_LEN}",
            body.len()
        );
    }
}
